use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

pub const WIFI_RING_SLOTS: usize = 32;
pub const WIFI_SNAP_LEN: usize = 768;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WifiHdr {
    pub rssi: i8,
    pub channel: u8,
    pub freq_mhz: u16,
    pub is_5ghz: bool,
    pub rate: u8,
    pub ts_ms: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WifiSlot {
    pub hdr: WifiHdr,
    pub len: u16,
    pub payload: [u8; WIFI_SNAP_LEN],
}

impl WifiSlot {
    pub const EMPTY: Self = Self {
        hdr: WifiHdr {
            rssi: 0,
            channel: 0,
            freq_mhz: 0,
            is_5ghz: false,
            rate: 0,
            ts_ms: 0,
        },
        len: 0,
        payload: [0; WIFI_SNAP_LEN],
    };

    pub fn payload(&self) -> &[u8] {
        &self.payload[..usize::from(self.len)]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Stored { truncated: bool },
    Full,
}

pub struct WifiRing {
    slots: [UnsafeCell<WifiSlot>; WIFI_RING_SLOTS],
    write: AtomicUsize,
    read: AtomicUsize,
}

// SPSC: wifi task pushes, main pops; slots are written before the write index is published.
unsafe impl Sync for WifiRing {}

impl WifiRing {
    pub const fn new() -> Self {
        Self {
            slots: [const { UnsafeCell::new(WifiSlot::EMPTY) }; WIFI_RING_SLOTS],
            write: AtomicUsize::new(0),
            read: AtomicUsize::new(0),
        }
    }

    pub fn try_push(&self, hdr: WifiHdr, src: &[u8], sig_len: usize) -> PushOutcome {
        let w = self.write.load(Ordering::Relaxed);
        let r = self.read.load(Ordering::Acquire);
        if w.wrapping_sub(r) >= WIFI_RING_SLOTS {
            return PushOutcome::Full;
        }
        let copy = src.len().min(sig_len).min(WIFI_SNAP_LEN);
        // SAFETY: `w - r < CAP`, so this slot is not visible to the consumer.
        unsafe {
            let slot = &mut *self.slots[w % WIFI_RING_SLOTS].get();
            slot.hdr = hdr;
            slot.len = copy as u16;
            if copy > 0 {
                slot.payload[..copy].copy_from_slice(&src[..copy]);
            }
        }
        self.write.store(w.wrapping_add(1), Ordering::Release);
        PushOutcome::Stored {
            truncated: copy < sig_len,
        }
    }

    pub fn try_pop(&self) -> Option<WifiSlot> {
        let r = self.read.load(Ordering::Relaxed);
        let w = self.write.load(Ordering::Acquire);
        if r == w {
            return None;
        }
        // SAFETY: `r != w`, so the producer has Release-published this slot.
        let slot = unsafe { *self.slots[r % WIFI_RING_SLOTS].get() };
        self.read.store(r.wrapping_add(1), Ordering::Release);
        Some(slot)
    }
}

impl Default for WifiRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hdr(ts_ms: u64) -> WifiHdr {
        WifiHdr {
            rssi: -40,
            channel: 6,
            freq_mhz: 2437,
            is_5ghz: false,
            rate: 2,
            ts_ms,
        }
    }

    #[test]
    fn empty_pop_is_none() {
        assert!(WifiRing::new().try_pop().is_none());
        assert!(WifiRing::default().try_pop().is_none());
    }

    #[test]
    fn push_pop_preserves_bytes_and_header() {
        let ring = WifiRing::new();
        let src = [1u8, 2, 3, 4];
        assert_eq!(
            ring.try_push(hdr(9), &src, src.len()),
            PushOutcome::Stored { truncated: false }
        );
        let slot = ring.try_pop().unwrap();
        assert_eq!(slot.hdr, hdr(9));
        assert_eq!(slot.payload(), &src);
        assert!(ring.try_pop().is_none());
    }

    #[test]
    fn full_drops_without_overwrite() {
        let ring = WifiRing::new();
        for i in 0..WIFI_RING_SLOTS {
            let b = [i as u8];
            assert_eq!(
                ring.try_push(hdr(i as u64), &b, 1),
                PushOutcome::Stored { truncated: false }
            );
        }
        assert_eq!(ring.try_push(hdr(99), &[0xff], 1), PushOutcome::Full);
        let first = ring.try_pop().unwrap();
        assert_eq!(first.hdr.ts_ms, 0);
        assert_eq!(first.payload(), &[0]);
    }

    #[test]
    fn wrap_reuses_slots() {
        let ring = WifiRing::new();
        for i in 0..WIFI_RING_SLOTS {
            assert!(matches!(
                ring.try_push(hdr(i as u64), &[i as u8], 1),
                PushOutcome::Stored { truncated: false }
            ));
        }
        assert_eq!(ring.try_pop().unwrap().payload(), &[0]);
        assert_eq!(
            ring.try_push(hdr(100), &[0xaa], 1),
            PushOutcome::Stored { truncated: false }
        );
        for i in 1..WIFI_RING_SLOTS {
            assert_eq!(ring.try_pop().unwrap().payload(), &[i as u8]);
        }
        assert_eq!(ring.try_pop().unwrap().payload(), &[0xaa]);
        assert!(ring.try_pop().is_none());
    }

    #[test]
    fn copies_min_of_src_sig_and_snap_and_flags_trunc() {
        let ring = WifiRing::new();
        let src = vec![7u8; WIFI_SNAP_LEN + 8];
        assert_eq!(
            ring.try_push(hdr(1), &src, src.len()),
            PushOutcome::Stored { truncated: true }
        );
        let slot = ring.try_pop().unwrap();
        assert_eq!(slot.payload().len(), WIFI_SNAP_LEN);
        assert_eq!(slot.payload()[0], 7);
        assert_eq!(
            ring.try_push(hdr(2), &[1, 2, 3], 1),
            PushOutcome::Stored { truncated: false }
        );
        assert_eq!(ring.try_pop().unwrap().payload(), &[1]);
        assert_eq!(
            ring.try_push(hdr(3), &[9, 8], 8),
            PushOutcome::Stored { truncated: true }
        );
        assert_eq!(ring.try_pop().unwrap().payload(), &[9, 8]);
        assert_eq!(
            ring.try_push(hdr(4), &[], 0),
            PushOutcome::Stored { truncated: false }
        );
        assert_eq!(ring.try_pop().unwrap().payload(), &[] as &[u8]);
        assert_eq!(WifiSlot::EMPTY.payload(), &[] as &[u8]);
        assert_eq!(WifiSlot::EMPTY.len, 0);
    }
}
