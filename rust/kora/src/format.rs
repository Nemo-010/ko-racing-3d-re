//! Parsers for the K.O. Racing 3D asset formats.
//!
//! Ported one-to-one from the MIDlet classes; the byte layouts are documented
//! in `tools/README.md`.  Everything is big endian and a `string` is a `u8`
//! length followed by that many Latin-1 bytes.

/// Cursor over a resource blob.
pub struct Reader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len() - self.pos
    }

    pub fn u8(&mut self) -> u8 {
        let value = self.data[self.pos];
        self.pos += 1;
        value
    }

    pub fn i8(&mut self) -> i8 {
        self.u8() as i8
    }

    pub fn u16(&mut self) -> u16 {
        let value = u16::from_be_bytes([self.data[self.pos], self.data[self.pos + 1]]);
        self.pos += 2;
        value
    }

    pub fn u32(&mut self) -> u32 {
        let value = u32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
        value
    }

    pub fn string(&mut self) -> String {
        let len = self.u8() as usize;
        let text = String::from_utf8_lossy(self.bytes(len)).into_owned();
        text
    }

    pub fn bytes(&mut self, count: usize) -> &'a [u8] {
        let slice = &self.data[self.pos..self.pos + count];
        self.pos += count;
        slice
    }
}

/// Strip counts, indices and bytes/100 floats out of a raw mesh.
#[derive(Clone)]
pub struct Model {
    pub positions: Vec<[f32; 3]>,
    pub texcoords: Vec<[f32; 2]>,
    pub strips: Vec<u8>,
    pub indices: Vec<u8>,
}

impl Model {
    pub fn parse(data: &[u8]) -> Option<Model> {
        if data.len() < 4 || &data[0..3] != b"\x00\x00\x00" {
            return None;
        }
        let mut r = Reader::new(data);
        r.pos = 3;
        let pos_scale: f32 = r.string().parse().ok()?;
        r.u8();
        let pos_bias: f32 = r.string().parse().ok()?;
        r.u8();
        let tex_scale: f32 = r.string().parse().ok()?;
        r.u8();
        let tex_bias: f32 = r.string().parse().ok()?;

        let count = r.u8() as usize;
        let pos_offset = 128.0 * pos_scale + pos_bias;
        let tex_offset = 128.0 * tex_scale + tex_bias;
        let mut positions = Vec::with_capacity(count);
        let mut texcoords = Vec::with_capacity(count);
        for _ in 0..count {
            let x = r.i8() as f32;
            let y = r.i8() as f32;
            let z = r.i8() as f32;
            let u = r.u8() as f32;
            let v = r.u8() as f32;
            positions.push([
                pos_scale * x + pos_offset,
                pos_scale * y + pos_offset,
                pos_scale * z + pos_offset,
            ]);
            texcoords.push([
                tex_scale * u + tex_offset,
                1.0 - (tex_scale * v + tex_offset),
            ]);
        }

        let strip_count = r.u8() as usize;
        let strips = r.bytes(strip_count).to_vec();
        let index_count = r.u16() as usize;
        let indices = r.bytes(index_count).to_vec();
        Some(Model {
            positions,
            texcoords,
            strips,
            indices,
        })
    }

    /// Triangulate the strips, alternating winding as the strip spec requires.
    pub fn triangles(&self) -> Vec<[usize; 3]> {
        let mut faces = Vec::new();
        let mut cursor = 0usize;
        for &length in &self.strips {
            let length = length as usize;
            let strip: Vec<usize> = self.indices[cursor..cursor + length]
                .iter()
                .map(|&i| i as usize)
                .collect();
            cursor += length;
            for i in 0..length.saturating_sub(2) {
                if i % 2 == 0 {
                    faces.push([strip[i], strip[i + 1], strip[i + 2]]);
                } else {
                    faces.push([strip[i + 1], strip[i], strip[i + 2]]);
                }
            }
        }
        faces
    }

    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut min = [f32::MAX; 3];
        let mut max = [f32::MIN; 3];
        for p in &self.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        (min, max)
    }
}

/// `.tl` - class `ar`.
///
/// The two 4-byte flag blocks are read in this order: `solid`, then `open`.
/// Despite the second block being easy to read as "walls", it is what the
/// MIDlet walks the road with (`bm.b(i)` -> `ar.b(i)`, used by the `bs`
/// flood), and on every shipped map it is the *drivable* side flag: for
/// `s.tl` the +X/-X sides are set and the +/-Y sides clear.  The first block
/// is only used for tile edge rendering and the minimap.
pub struct Tile {
    pub name: String,
    pub texture: String,
    pub variant: u8,
    pub solid: [bool; 4],
    pub open_sides: [bool; 4],
}

impl Tile {
    pub fn parse(data: &[u8]) -> Option<Tile> {
        let mut r = Reader::new(data);
        let name = r.string();
        let texture = r.string();
        let variant = r.u8();

        for _ in 0..r.u8() {
            r.u8();
            r.u8();
        }
        for _ in 0..r.u8() {
            r.u8();
        }
        for _ in 0..r.u8() {
            if r.u8() >= 100 {
                r.u8();
            }
        }

        let mut solid = [false; 4];
        for side in solid.iter_mut() {
            *side = r.u8() != 0;
        }
        let open_sides = if r.u8() != 0 {
            let mut open = [false; 4];
            for side in open.iter_mut() {
                *side = r.u8() != 0;
            }
            open
        } else {
            solid
        };
        r.u8();
        Some(Tile {
            name,
            texture,
            variant,
            solid,
            open_sides,
        })
    }
}

/// One placed track cell: `tile` plus mid/high detail references.
pub struct MapCell {
    pub flags: u8,
    pub tile: Option<(u8, u8)>,
    pub mid: Vec<(u8, u8)>,
    pub high: Vec<(u8, u8)>,
}

/// `.map` - class `bs`.
pub struct Map {
    pub width: u8,
    pub height: u8,
    pub cells: Vec<Vec<MapCell>>,
    pub start: (u8, u8),
    pub finish: (u8, u8),
    pub checkpoints: Vec<(u8, u8)>,
}

impl Map {
    pub fn parse(data: &[u8]) -> Option<Map> {
        let mut r = Reader::new(data);
        let width = r.u8();
        let height = r.u8();
        let mut cells = Vec::with_capacity(height as usize);
        for _ in 0..height {
            let mut row = Vec::with_capacity(width as usize);
            for _ in 0..width {
                let flags = r.u8();
                let mut payloads = Vec::new();
                for bit in 0..7 {
                    if flags & (1 << bit) != 0 {
                        payloads.push((r.u8(), r.u8()));
                    }
                }
                let tile = if flags & 1 != 0 { Some(payloads[0]) } else { None };
                let mid = payloads.get(1..4).map(<[(u8, u8)]>::to_vec).unwrap_or_default();
                let high = payloads.get(4..).map(<[(u8, u8)]>::to_vec).unwrap_or_default();
                row.push(MapCell {
                    flags,
                    tile,
                    mid,
                    high,
                });
            }
            cells.push(row);
        }
        let start = (r.u8(), r.u8());
        let finish = (r.u8(), r.u8());
        let count = r.u8();
        let checkpoints = (0..count).map(|_| (r.u8(), r.u8())).collect();
        Some(Map {
            width,
            height,
            cells,
            start,
            finish,
            checkpoints,
        })
    }

    pub fn cell(&self, x: i32, y: i32) -> Option<&MapCell> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            return None;
        }
        Some(&self.cells[y as usize][x as usize])
    }
}

/// `.car` - class `ba`.
pub struct Car {
    pub model: String,
    pub low_model: String,
    pub texture: String,
    pub name: String,
    pub stats: [u8; 4],
}

impl Car {
    pub fn parse(data: &[u8]) -> Option<Car> {
        let mut r = Reader::new(data);
        let model = r.string();
        let low_model = r.string();
        let texture = r.string();
        let name = r.string();
        let stats = [r.u8(), r.u8(), r.u8(), r.u8()];
        Some(Car {
            model,
            low_model,
            texture,
            name,
            stats,
        })
    }
}

/// `.ob` - class `ai`.
pub struct ObjectDef {
    pub model: String,
    pub texture: String,
    pub flag: bool,
}

impl ObjectDef {
    pub fn parse(data: &[u8]) -> Option<ObjectDef> {
        let mut r = Reader::new(data);
        let model = r.string();
        let texture = r.string();
        let flag = r.u8() != 0;
        Some(ObjectDef {
            model,
            texture,
            flag,
        })
    }
}

/// `.md` - class `bc`.
pub struct MidDetail {
    pub model: String,
    pub texture: String,
}

impl MidDetail {
    pub fn parse(data: &[u8]) -> Option<MidDetail> {
        let mut r = Reader::new(data);
        Some(MidDetail {
            model: r.string(),
            texture: r.string(),
        })
    }
}

/// One placement inside a `.hd`.
pub struct HighDetailEntry {
    pub kind: u8,
    /// `byte/100 - 1`, in half-tile units (`ar.b` = 7 scales it to world).
    pub position: [f32; 3],
    pub scale: [f32; 3],
    pub scale2: [f32; 3],
}

/// `.hd` - class `bp`.
pub struct HighDetail {
    pub entries: Vec<HighDetailEntry>,
}

impl HighDetail {
    pub fn parse(data: &[u8]) -> Option<HighDetail> {
        let mut r = Reader::new(data);
        let count = r.u8() as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            let kind = r.u8();
            let read3 = |r: &mut Reader| {
                [
                    r.u8() as f32 / 100.0 - 1.0,
                    r.u8() as f32 / 100.0 - 1.0,
                    r.u8() as f32 / 100.0 - 1.0,
                ]
            };
            let position = read3(&mut r);
            let read_scales = |r: &mut Reader| {
                [r.u8() as f32 / 100.0, r.u8() as f32 / 100.0, r.u8() as f32 / 100.0]
            };
            let scale = read_scales(&mut r);
            let scale2 = read_scales(&mut r);
            entries.push(HighDetailEntry {
                kind,
                position,
                scale,
                scale2,
            });
        }
        Some(HighDetail { entries })
    }
}

/// `.tab` - class `g`.
pub struct FontTable {
    pub codes: Vec<u16>,
    pub glyphs: Vec<u8>,
}

impl FontTable {
    pub fn parse(data: &[u8]) -> Option<FontTable> {
        let mut r = Reader::new(data);
        let has_index = r.u8() != 0;
        let count = r.u8() as usize;
        let mut codes = Vec::with_capacity(count);
        let mut glyphs = Vec::with_capacity(count);
        for i in 0..count {
            codes.push(r.u8() as u16 | ((r.u8() as u16) << 8));
            glyphs.push(if has_index { r.u8() } else { i as u8 });
        }
        Some(FontTable { codes, glyphs })
    }

    pub fn index(&self, ch: char) -> usize {
        let code = ch as u16;
        match self.codes.binary_search(&code) {
            Ok(pos) => self.glyphs[pos] as usize,
            Err(_) => 0,
        }
    }
}

/// Font metrics file (`/fonts/<name>`, no extension) - class `p`.
pub struct Font {
    pub widths: Vec<u8>,
    pub cell_height: u8,
    pub table: Option<FontTable>,
}

impl Font {
    pub fn parse(data: &[u8], table: Option<FontTable>) -> Option<Font> {
        let mut r = Reader::new(data);
        let count = r.u8() as usize;
        let widths = r.bytes(count).to_vec();
        let cell_height = r.u8();
        Some(Font {
            widths,
            cell_height,
            table,
        })
    }

    pub fn glyph_for(&self, ch: char) -> usize {
        match &self.table {
            Some(table) => table.index(ch),
            None => ch as usize,
        }
    }

    pub fn width(&self, ch: char) -> u8 {
        let glyph = self.glyph_for(ch);
        *self.widths.get(glyph).unwrap_or(&0)
    }

    pub fn text_width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.width(c) as f32).sum()
    }

    /// `(x, y, w, h)` for every glyph in the atlas.
    pub fn layout(&self, atlas_width: u32) -> Vec<(u32, u32, u32, u32)> {
        let mut rects = Vec::with_capacity(self.widths.len());
        let (mut x, mut y) = (0u32, 0u32);
        for &width in &self.widths {
            let width = width as u32;
            if x + width > atlas_width {
                x = 0;
                y += self.cell_height as u32;
            }
            rects.push((x, y, width, self.cell_height as u32));
            x += width;
        }
        rects
    }
}
