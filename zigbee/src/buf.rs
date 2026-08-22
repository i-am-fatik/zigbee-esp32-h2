pub struct Writer<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> Writer<'a> {
    pub fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn u8(&mut self, v: u8) -> &mut Self {
        self.buf[self.pos] = v;
        self.pos += 1;
        self
    }

    pub fn u16(&mut self, v: u16) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn u32(&mut self, v: u32) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn u64(&mut self, v: u64) -> &mut Self {
        self.bytes(&v.to_le_bytes())
    }

    pub fn bytes(&mut self, v: &[u8]) -> &mut Self {
        self.buf[self.pos..self.pos + v.len()].copy_from_slice(v);
        self.pos += v.len();
        self
    }

    pub fn len(&self) -> usize {
        self.pos
    }

    pub fn written(&self) -> &[u8] {
        &self.buf[..self.pos]
    }
}

pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.buf.len() - self.pos
    }

    pub fn u8(&mut self) -> Option<u8> {
        let v = *self.buf.get(self.pos)?;
        self.pos += 1;
        Some(v)
    }

    pub fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.array::<2>()?))
    }

    pub fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.array::<4>()?))
    }

    pub fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.array::<8>()?))
    }

    pub fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        let slice = self.buf.get(self.pos..self.pos + N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        self.pos += N;
        Some(out)
    }

    pub fn skip(&mut self, n: usize) -> Option<()> {
        if self.pos + n > self.buf.len() {
            return None;
        }
        self.pos += n;
        Some(())
    }

    pub fn rest(&self) -> &'a [u8] {
        &self.buf[self.pos..]
    }
}

impl<'a> Writer<'a> {
    pub fn set(&mut self, pos: usize, value: u8) {
        self.buf[pos] = value;
    }

    pub fn truncate(&mut self, pos: usize) {
        self.pos = pos;
    }
}
