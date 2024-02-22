#[derive(Clone, Debug, Default)]
pub struct Samples(Vec<f32>);

impl Samples {
    pub fn new(data: Vec<f32>) -> Self {
        Samples(data)
    }

    pub fn add(&mut self, other: &Self) {
        self.0.extend((self.0.len()..other.0.len()).map(|_| 0.0));
        self.0
            .iter_mut()
            .zip(other.0.iter())
            .for_each(|(a, b)| *a += b);
    }

    pub fn subtract(&mut self, other: &Self) {
        self.0.extend((self.0.len()..other.0.len()).map(|_| 0.0));
        self.0
            .iter_mut()
            .zip(other.0.iter())
            .for_each(|(a, b)| *a -= b);
    }

    pub fn truncate(&mut self, len: usize) {
        self.0.truncate(len);
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn to_vec(self) -> Vec<f32> {
        self.0
    }
}

impl From<Samples> for Vec<f32> {
    fn from(value: Samples) -> Self {
        value.0
    }
}
