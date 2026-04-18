pub struct Buffer {
    width: usize,
    height: usize,
    buf: Vec<u8>,
}

impl Buffer {
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            buf: vec![0; width * height],
        }
    }

    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn at(&self, x: usize, y: usize) -> u8 {
        self.buf[y * self.width + x]
    }

    pub fn at_mut(&mut self, x: usize, y: usize) -> &mut u8 {
        &mut self.buf[y * self.width + x]
    }
}

fn hex_anchor(q: i32, r: i32) -> (i32, i32) {
    // supposing (0, 0) -> (0, 0)
    //
    // (+1, +0) -> (+9, +2)
    // (+0, +1) -> (+9, -2)

    ((q + r) * 9, (q - r) * 2)
}

fn hex_anchor_shifted(q: i32, r: i32) -> (i32, i32) {
    // supposing (-3, 1) -> (0, 0)
    //
    // (+1, +0) -> (+9, +2)
    // (+0, +1) -> (+9, -2)

    let q = q + 3;
    let r = r - 1;

    ((q + r) * 9, (q - r) * 2)
}

pub fn draw_field() {
    "  _______   \n\
      /       \\ \n\
     /         \\\n\
     \\         /\n\
      \\_______/ \n\
     ";

    let buf = Buffer::new(48, 21);
}
