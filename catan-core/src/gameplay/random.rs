use rand::{
    Rng, SeedableRng,
    rngs::{SmallRng, ThreadRng},
};

pub enum GameRandom {
    Thread(ThreadRng),
    Seeded(SmallRng),
}

impl GameRandom {
    pub fn thread() -> Self {
        Self::Thread(rand::rng())
    }

    pub fn seeded(seed: u64) -> Self {
        Self::Seeded(SmallRng::seed_from_u64(seed))
    }

    pub fn with_rng<T>(&mut self, f: impl FnOnce(&mut dyn Rng) -> T) -> T {
        match self {
            Self::Thread(rng) => f(rng),
            Self::Seeded(rng) => f(rng),
        }
    }
}

impl Default for GameRandom {
    fn default() -> Self {
        Self::thread()
    }
}

impl std::fmt::Debug for GameRandom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Thread(_) => f.write_str("GameRandom::Thread"),
            Self::Seeded(_) => f.write_str("GameRandom::Seeded"),
        }
    }
}
