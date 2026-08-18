#[cfg(not(feature = "alloc"))]
pub(crate) const MAX_WINDOW_SIZE: usize = 1000;

#[cfg(feature = "alloc")]
#[derive(Debug, Clone)]
pub(crate) struct RingBuffer {
    buffer: alloc::vec::Vec<f64>,
    capacity: usize,
    head: usize,
    count: usize,
    running_sum: f64,
    compensation: f64,
}

#[cfg(not(feature = "alloc"))]
#[derive(Debug, Clone, Copy)]
pub(crate) struct RingBuffer {
    buffer: [f64; MAX_WINDOW_SIZE],
    capacity: usize,
    head: usize,
    count: usize,
    running_sum: f64,
    compensation: f64,
}

impl RingBuffer {
    #[cfg(feature = "alloc")]
    pub fn new(capacity: usize) -> Self {
        Self {
            buffer: alloc::vec![0.0; capacity],
            capacity,
            head: 0,
            count: 0,
            running_sum: 0.0,
            compensation: 0.0,
        }
    }

    #[cfg(not(feature = "alloc"))]
    pub fn new(capacity: usize) -> Self {
        assert!(
            capacity <= MAX_WINDOW_SIZE,
            "capacity {capacity} exceeds MAX_WINDOW_SIZE ({MAX_WINDOW_SIZE})"
        );
        Self {
            buffer: [0.0; MAX_WINDOW_SIZE],
            capacity,
            head: 0,
            count: 0,
            running_sum: 0.0,
            compensation: 0.0,
        }
    }

    #[inline]
    fn kahan_add(&mut self, value: f64) {
        let y = value - self.compensation;
        let t = self.running_sum + y;
        self.compensation = (t - self.running_sum) - y;
        self.running_sum = t;
    }

    #[inline]
    pub fn push(&mut self, value: f64) {
        if self.capacity == 0 {
            return;
        }

        self.kahan_add(-self.buffer[self.head]);
        self.buffer[self.head] = value;
        self.kahan_add(value);
        self.head += 1;
        if self.head == self.capacity {
            self.head = 0;
        }

        if self.count < self.capacity {
            self.count += 1;
        }
    }

    #[inline]
    pub fn sum(&self) -> f64 {
        self.running_sum
    }

    #[inline]
    pub fn mean(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.running_sum / self.count as f64)
        }
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.count
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.count == 0
    }

    #[cfg(test)]
    fn is_full(&self) -> bool {
        self.count == self.capacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn test_new_buffer_is_empty() {
        let buf = RingBuffer::new(5);
        assert!(buf.is_empty());
        assert_eq!(buf.len(), 0);
        assert!(!buf.is_full());
        assert_relative_eq!(buf.sum(), 0.0, epsilon = 1e-10);
        assert!(buf.mean().is_none());
    }

    #[test]
    fn test_push_single_value() {
        let mut buf = RingBuffer::new(5);
        buf.push(10.0);
        assert_eq!(buf.len(), 1);
        assert_relative_eq!(buf.sum(), 10.0, epsilon = 1e-10);
        assert_relative_eq!(buf.mean().unwrap(), 10.0, epsilon = 1e-10);
    }

    #[test]
    fn test_push_until_full() {
        let mut buf = RingBuffer::new(3);
        buf.push(1.0);
        buf.push(2.0);
        buf.push(3.0);
        assert!(buf.is_full());
        assert_relative_eq!(buf.sum(), 6.0, epsilon = 1e-10);
    }

    #[test]
    fn test_push_overflow_evicts_oldest() {
        let mut buf = RingBuffer::new(3);
        buf.push(1.0);
        buf.push(2.0);
        buf.push(3.0);
        buf.push(4.0);
        assert_relative_eq!(buf.sum(), 9.0, epsilon = 1e-10);
    }

    #[test]
    fn test_multiple_overflows() {
        let mut buf = RingBuffer::new(3);
        buf.push(1.0);
        buf.push(2.0);
        buf.push(3.0);
        buf.push(4.0);
        buf.push(5.0);
        buf.push(6.0);
        assert_relative_eq!(buf.sum(), 15.0, epsilon = 1e-10);
    }

    #[test]
    fn test_zero_capacity() {
        let mut buf = RingBuffer::new(0);
        buf.push(10.0);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_negative_values() {
        let mut buf = RingBuffer::new(3);
        buf.push(-1.0);
        buf.push(2.0);
        buf.push(-3.0);
        assert_relative_eq!(buf.sum(), -2.0, epsilon = 1e-10);
        buf.push(4.0);
        assert_relative_eq!(buf.sum(), 3.0, epsilon = 1e-10);
    }

    #[test]
    fn test_kahan_precision_over_many_pushes() {
        let mut buf = RingBuffer::new(100);
        for i in 0..1_000_000 {
            buf.push(0.1 + (i % 100) as f64 * 0.001);
        }
        let mut expected = 0.0_f64;
        for i in (1_000_000 - 100)..1_000_000 {
            expected += 0.1 + (i % 100) as f64 * 0.001;
        }
        assert_relative_eq!(buf.sum(), expected, epsilon = 1e-9);
    }
}
