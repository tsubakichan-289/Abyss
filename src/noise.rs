#[derive(Clone, Copy, Debug)]
pub struct Perlin2D {
    perm: [u8; 512],
}

impl Perlin2D {
    pub fn new(seed: u64) -> Self {
        let mut p = [0u8; 256];
        for (i, value) in p.iter_mut().enumerate() {
            *value = i as u8;
        }

        let mut state = seed;
        for i in (1..256).rev() {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = ((state >> 32) as usize) % (i + 1);
            p.swap(i, j);
        }

        let mut perm = [0u8; 512];
        for i in 0..512 {
            perm[i] = p[i & 255];
        }

        Self { perm }
    }

    pub fn noise(&self, x: f64, y: f64) -> f64 {
        let xi = x.floor() as i32 & 255;
        let yi = y.floor() as i32 & 255;
        let xf = x - x.floor();
        let yf = y - y.floor();

        let u = fade(xf);
        let v = fade(yf);

        let xi_u = xi as usize;
        let yi_u = yi as usize;
        let xi1 = (xi_u + 1) & 255;
        let yi1 = (yi_u + 1) & 255;

        let aa = self.perm[(self.perm[xi_u] as usize + yi_u) & 255];
        let ab = self.perm[(self.perm[xi_u] as usize + yi1) & 255];
        let ba = self.perm[(self.perm[xi1] as usize + yi_u) & 255];
        let bb = self.perm[(self.perm[xi1] as usize + yi1) & 255];

        let x1 = lerp(grad(aa, xf, yf), grad(ba, xf - 1.0, yf), u);
        let x2 = lerp(grad(ab, xf, yf - 1.0), grad(bb, xf - 1.0, yf - 1.0), u);
        lerp(x1, x2, v)
    }

    pub fn noise01(&self, x: f64, y: f64) -> f64 {
        (self.noise(x, y) + 1.0) * 0.5
    }
}

fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + t * (b - a)
}

fn grad(hash: u8, x: f64, y: f64) -> f64 {
    match hash & 7 {
        0 => x + y,
        1 => -x + y,
        2 => x - y,
        3 => -x - y,
        4 => x,
        5 => -x,
        6 => y,
        _ => -y,
    }
}
