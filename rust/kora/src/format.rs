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

    pub fn i32(&mut self) -> i32 {
        let value = i32::from_be_bytes([
            self.data[self.pos],
            self.data[self.pos + 1],
            self.data[self.pos + 2],
            self.data[self.pos + 3],
        ]);
        self.pos += 4;
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

/// Triangle soup a tile uses for height sampling (class `a`).
///
/// Vertices are three bytes each divided by 100; the game wraps anything above
/// 2.0 by -2.56, which only ever fires for the top of the byte range.  The
/// local frame is the same `[0,1]^2` square `bm.a(float, float)` rotates a
/// sample point into, and the third component is *negated* when the triangle
/// is built, so world height is `-z * TILE` (14).  A zero vertex count ends
/// the stream with no triangle list at all.
#[derive(Clone)]
pub struct Collision {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[usize; 3]>,
}

impl Collision {
    fn parse(r: &mut Reader) -> Collision {
        let count = r.u8() as usize;
        if count == 0 {
            return Collision {
                vertices: Vec::new(),
                triangles: Vec::new(),
            };
        }
        let mut vertices = Vec::with_capacity(count);
        for _ in 0..count {
            let mut v = [0.0f32; 3];
            for component in v.iter_mut() {
                let mut value = r.u8() as f32 / 100.0;
                if value > 2.0 {
                    value -= 2.56;
                }
                *component = value;
            }
            vertices.push(v);
        }
        let triangles = (0..r.u8() as usize)
            .map(|_| [r.u8() as usize, r.u8() as usize, r.u8() as usize])
            .collect();
        Collision {
            vertices,
            triangles,
        }
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
    /// Height-sampling mesh; most tiles ship none, and the game then treats
    /// the whole cell as flat at zero.
    pub collision: Option<Collision>,
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
        let collision = if r.remaining() > 0 {
            Some(Collision::parse(&mut r))
        } else {
            None
        };
        Some(Tile {
            name,
            texture,
            variant,
            solid,
            open_sides,
            collision,
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

/// One level of the career table (`.000`).
pub struct CampaignLevel {
    pub x: i16,
    pub y: i16,
    pub name: String,
    pub map: String,
    /// Bitmask of the game modes offered for this level.
    pub modes: u8,
    pub flags: u8,
    pub unlocked: u8,
}

/// One race entry of the career table: a mode plus the offset of its setup
/// inside the matching `.001`.
pub struct RaceRecord {
    pub mode: u8,
    pub level: u8,
    pub values: [i32; 3],
}

/// `.000` - class `u`.
///
/// ```text
/// u8    level_count
/// repeat: i16 x, i16 y, str name, str map, u8 modes, u8 flags, u8 unlocked
/// u8    unlock_count_minus_one ; unlock_count = read + 1
/// repeat (unlock_count - 1): i32
/// u8    download_count
/// repeat: i32, str
/// u8    record_count
/// repeat: u8 mode, u8 level, i32, i32, i32
/// ```
pub struct Campaign {
    pub levels: Vec<CampaignLevel>,
    pub unlock: Vec<i32>,
    pub downloads: Vec<(i32, String)>,
    pub records: Vec<RaceRecord>,
}

impl Campaign {
    pub fn parse(data: &[u8]) -> Option<Campaign> {
        let mut r = Reader::new(data);
        let count = r.u8() as usize;
        let mut levels = Vec::with_capacity(count);
        for _ in 0..count {
            levels.push(CampaignLevel {
                x: r.u16() as i16,
                y: r.u16() as i16,
                name: r.string(),
                map: r.string(),
                modes: r.u8(),
                flags: r.u8(),
                unlocked: r.u8(),
            });
        }
        let unlock_count = r.u8() as usize + 1;
        let unlock = (0..unlock_count.saturating_sub(1)).map(|_| r.i32()).collect();
        let download_count = r.u8() as usize;
        let downloads = (0..download_count).map(|_| (r.i32(), r.string())).collect();
        let record_count = r.u8() as usize;
        let records = (0..record_count)
            .map(|_| RaceRecord {
                mode: r.u8(),
                level: r.u8(),
                values: [r.i32(), r.i32(), r.i32()],
            })
            .collect();
        Some(Campaign {
            levels,
            unlock,
            downloads,
            records,
        })
    }
}

/// A per-race setup read from `<campaign>.001` at a record's offset.
///
/// Every game mode stores its setup with its own layout, taken from the
/// reader the MIDlet uses for that mode:
///
/// ```text
/// mode 0 / 4  (cu.b)  u8 laps, u8 theme, u8 flag, u8 car, u8 param, u8 opponents
/// mode 1      (r.b)   u8 theme, u8 flag, u8 car, u8 param, u8 opponents  (one lap)
/// mode 2 / 6  (dk.b)  u8 theme, u8 flag, u8 laps, u8 car, i32 time_limit
/// mode 3      (cd.b)  u8 theme, u8 flag, u8 car, u8 param, u8 opponents  (laps := opponents)
/// mode 5      (bv.b)  u8 theme, u8 flag, u8 laps, u8 car, i32 time_limit
/// ```
///
/// `theme` picks the tile/texture variant (`bm` and `bp` compare it with 3,
/// and `dk` forces it to 3 for `8a.map`).  `car` is the player's car index,
/// except that values >= 50 mark the deluxe time-attack entries, where the
/// trailing `i32` is the time limit instead.
pub struct RaceConfig {
    pub mode: u8,
    pub laps: u32,
    pub theme: u8,
    pub flag: bool,
    pub car: u8,
    pub opponents: u32,
    pub param: Option<u8>,
    pub time_limit: Option<i32>,
}

impl RaceConfig {
    /// Modes that are a race against opponents rather than a solo time trial.
    pub const RACE_MODES: [u8; 4] = [0, 1, 3, 4];

    pub fn is_race(&self) -> bool {
        Self::RACE_MODES.contains(&self.mode)
    }

    pub fn parse(data: &[u8], offset: usize, mode: u8) -> Option<RaceConfig> {
        let mut r = Reader::new(data);
        r.pos = offset;
        if offset >= data.len() {
            return None;
        }
        let config = match mode {
            0 | 4 => {
                let laps = r.u8();
                let theme = r.u8();
                let flag = r.u8() != 0;
                let car = r.u8();
                let param = r.u8();
                let opponents = r.u8();
                RaceConfig {
                    mode,
                    laps: laps as u32,
                    theme,
                    flag,
                    car,
                    opponents: opponents as u32,
                    param: Some(param),
                    time_limit: None,
                }
            }
            1 => {
                let theme = r.u8();
                let flag = r.u8() != 0;
                let car = r.u8();
                let param = r.u8();
                let opponents = r.u8();
                RaceConfig {
                    mode,
                    laps: 1,
                    theme,
                    flag,
                    car,
                    opponents: opponents as u32,
                    param: Some(param),
                    time_limit: None,
                }
            }
            2 | 5 | 6 => {
                let theme = r.u8();
                let flag = r.u8() != 0;
                let laps = r.u8();
                let car = r.u8();
                let time_limit = r.i32();
                RaceConfig {
                    mode,
                    laps: laps as u32,
                    theme,
                    flag,
                    car,
                    opponents: 0,
                    param: None,
                    time_limit: Some(time_limit),
                }
            }
            3 => {
                let theme = r.u8();
                let flag = r.u8() != 0;
                let car = r.u8();
                let param = r.u8();
                let opponents = r.u8();
                RaceConfig {
                    mode,
                    laps: opponents as u32,
                    theme,
                    flag,
                    car,
                    opponents: opponents as u32,
                    param: Some(param),
                    time_limit: None,
                }
            }
            _ => return None,
        };
        Some(config)
    }
}

/// `.bck` - class `al` (method `q(int)`).
///
/// ```text
/// str   texture            # looked up as /images/<name>.jpg, then .png
/// u32   colour[4]          # only the middle two are kept by the MIDlet
/// u8    detail
/// i32   scale_a            # stored as an integer, divided by 1000
/// i32   scale_b
/// ```
pub struct Background {
    pub texture: String,
    pub colours: [u32; 4],
    pub detail: u8,
    pub scale_a: f32,
    pub scale_b: f32,
}

impl Background {
    pub fn parse(data: &[u8]) -> Option<Background> {
        let mut r = Reader::new(data);
        let texture = r.string();
        let colours = [r.u32(), r.u32(), r.u32(), r.u32()];
        let detail = r.u8();
        let scale_a = r.i32() as f32 / 1000.0;
        let scale_b = r.i32() as f32 / 1000.0;
        Some(Background {
            texture,
            colours,
            detail,
            scale_a,
            scale_b,
        })
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
