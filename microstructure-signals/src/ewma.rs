#[derive(Debug, Clone, Copy)]
pub(crate) struct EwmaSmoothing {
    half_life: f64,
    lambda: f64,
    alpha: f64,
    smoothed: f64,
    count: usize,
}

impl EwmaSmoothing {
    pub fn new(half_life: f64) -> Self {
        assert!(
            half_life > 0.0 && half_life.is_finite(),
            "half_life must be finite and positive, got {half_life}"
        );
        let lambda = crate::math::exp(-crate::math::ln(2.0) / half_life);
        Self {
            half_life,
            lambda,
            alpha: 1.0 - lambda,
            smoothed: 0.0,
            count: 0,
        }
    }

    #[inline]
    pub fn update(&mut self, raw: f64) -> f64 {
        if self.count == 0 {
            self.smoothed = raw;
        } else {
            self.smoothed = self.lambda * self.smoothed + self.alpha * raw;
        }
        self.count += 1;
        self.smoothed
    }

    #[inline]
    pub fn half_life(&self) -> f64 {
        self.half_life
    }
}
