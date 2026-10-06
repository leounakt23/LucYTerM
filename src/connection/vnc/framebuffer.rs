//! RFB framebuffer + encoding decoders (Prompt 4.3).
//!
//! The client advertises only what it decodes — Raw, CopyRect, RRE, Hextile
//! (see [`SUPPORTED_ENCODINGS`]) — so servers never send anything else;
//! unknown ids fail loudly as [`VncError::UnsupportedEncoding`] instead of
//! corrupting the screen. Pixels land in [`Framebuffer`] as `0xFFRRGGBB` and
//! export to RGBA bytes for the wgpu texture in the viewer widget.
//!
//! Decoders stream from an async reader (rect payloads arrive off the
//! socket); only Raw buffers a full rect, everything else applies in place.

use tokio::io::{AsyncRead, AsyncReadExt};

use super::VncError;

/// Encodings we advertise via `SetEncodings` (order = preference).
pub const SUPPORTED_ENCODINGS: [i32; 4] = [
    ENCODING_HEXTILE,
    ENCODING_RRE,
    ENCODING_COPYRECT,
    ENCODING_RAW,
];

pub const ENCODING_RAW: i32 = 0;
pub const ENCODING_COPYRECT: i32 = 1;
pub const ENCODING_RRE: i32 = 2;
pub const ENCODING_HEXTILE: i32 = 5;

/// Negotiated pixel format (we always request 32-bit LE truecolor).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelFormat {
    pub bits_per_pixel: u8,
    pub depth: u8,
    pub big_endian: bool,
    pub true_color: bool,
    pub red_max: u16,
    pub green_max: u16,
    pub blue_max: u16,
    pub red_shift: u8,
    pub green_shift: u8,
    pub blue_shift: u8,
}

impl PixelFormat {
    /// Our `SetPixelFormat`: 32bpp LE XRGB truecolor.
    pub fn xrgb32() -> Self {
        Self {
            bits_per_pixel: 32,
            depth: 24,
            big_endian: false,
            true_color: true,
            red_max: 255,
            green_max: 255,
            blue_max: 255,
            red_shift: 16,
            green_shift: 8,
            blue_shift: 0,
        }
    }

    pub fn bytes_per_pixel(&self) -> usize {
        (self.bits_per_pixel as usize / 8).max(1)
    }

    /// 16-byte wire encoding (`SetPixelFormat` body).
    pub fn to_wire(&self) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0] = self.bits_per_pixel;
        out[1] = self.depth;
        out[2] = u8::from(self.big_endian);
        out[3] = u8::from(self.true_color);
        out[4..6].copy_from_slice(&self.red_max.to_be_bytes());
        out[6..8].copy_from_slice(&self.green_max.to_be_bytes());
        out[8..10].copy_from_slice(&self.blue_max.to_be_bytes());
        out[10] = self.red_shift;
        out[11] = self.green_shift;
        out[12] = self.blue_shift;
        out
    }

    /// Parse a 16-byte `ServerInit` pixel format.
    pub fn parse(raw: &[u8; 16]) -> Self {
        Self {
            bits_per_pixel: raw[0],
            depth: raw[1],
            big_endian: raw[2] != 0,
            true_color: raw[3] != 0,
            red_max: u16::from_be_bytes([raw[4], raw[5]]),
            green_max: u16::from_be_bytes([raw[6], raw[7]]),
            blue_max: u16::from_be_bytes([raw[8], raw[9]]),
            red_shift: raw[10],
            green_shift: raw[11],
            blue_shift: raw[12],
        }
    }

    /// Decode one pixel from wire bytes into `0xFFRRGGBB`.
    pub fn decode_pixel(&self, bytes: &[u8]) -> Result<u32, VncError> {
        if !self.true_color {
            return Err(VncError::protocol(
                "palette pixel formats are not supported",
            ));
        }
        let raw: u32 = match self.bits_per_pixel {
            8 => u32::from(bytes.first().copied().unwrap_or(0)),
            16 => {
                let pair = [
                    bytes.first().copied().unwrap_or(0),
                    bytes.get(1).copied().unwrap_or(0),
                ];
                if self.big_endian {
                    u32::from(u16::from_be_bytes(pair))
                } else {
                    u32::from(u16::from_le_bytes(pair))
                }
            },
            32 => {
                let quad = [
                    bytes.first().copied().unwrap_or(0),
                    bytes.get(1).copied().unwrap_or(0),
                    bytes.get(2).copied().unwrap_or(0),
                    bytes.get(3).copied().unwrap_or(0),
                ];
                if self.big_endian {
                    u32::from_be_bytes(quad)
                } else {
                    u32::from_le_bytes(quad)
                }
            },
            other => return Err(VncError::protocol(format!("bad pixel size: {other}"))),
        };
        let channel = |max: u16, shift: u8| -> u8 {
            if max == 0 {
                return 0;
            }
            (((raw >> shift) & u32::from(max)) * 255 / u32::from(max)) as u8
        };
        let r = channel(self.red_max, self.red_shift);
        let g = channel(self.green_max, self.green_shift);
        let b = channel(self.blue_max, self.blue_shift);
        Ok(0xFF00_0000 | (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
    }
}

/// One `FramebufferUpdate` rectangle header (12 bytes on the wire).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectHeader {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
    pub encoding: i32,
}

impl RectHeader {
    pub fn parse(raw: &[u8; 12]) -> Self {
        Self {
            x: u16::from_be_bytes([raw[0], raw[1]]),
            y: u16::from_be_bytes([raw[2], raw[3]]),
            width: u16::from_be_bytes([raw[4], raw[5]]),
            height: u16::from_be_bytes([raw[6], raw[7]]),
            encoding: i32::from_be_bytes([raw[8], raw[9], raw[10], raw[11]]),
        }
    }
}

/// Live pixel store (`0xFFRRGGBB` per cell).
#[derive(Debug, Clone)]
pub struct Framebuffer {
    width: u16,
    height: u16,
    pixels: Vec<u32>,
}

impl Framebuffer {
    pub fn new(width: u16, height: u16) -> Self {
        Self {
            width,
            height,
            pixels: vec![0xFF00_0000; width as usize * height as usize],
        }
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    /// Resize (e.g. remote `DesktopSize` change): content is dropped.
    pub fn resize(&mut self, width: u16, height: u16) {
        self.width = width;
        self.height = height;
        self.pixels = vec![0xFF00_0000; width as usize * height as usize];
    }

    pub fn get(&self, x: u16, y: u16) -> Option<u32> {
        self.pixels
            .get(y as usize * self.width as usize + x as usize)
            .copied()
    }

    pub fn set(&mut self, x: u16, y: u16, pixel: u32) {
        if x < self.width && y < self.height {
            self.pixels[y as usize * self.width as usize + x as usize] = pixel;
        }
    }

    fn fill_rect(&mut self, x: u16, y: u16, width: u16, height: u16, pixel: u32) {
        for row in y..y.saturating_add(height) {
            for col in x..x.saturating_add(width) {
                self.set(col, row, pixel);
            }
        }
    }

    /// RGBA bytes for the wgpu texture upload (row-major).
    pub fn to_rgba_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.pixels.len() * 4);
        for pixel in &self.pixels {
            out.push(((pixel >> 16) & 0xFF) as u8);
            out.push(((pixel >> 8) & 0xFF) as u8);
            out.push((pixel & 0xFF) as u8);
            out.push(((pixel >> 24) & 0xFF) as u8);
        }
        out
    }

    /// Snapshot for the viewer widget (generation assigned by the caller).
    pub fn snapshot(&self, generation: u64) -> FrameSnapshot {
        FrameSnapshot {
            generation,
            width: self.width,
            height: self.height,
            rgba: self.to_rgba_bytes(),
        }
    }
}

/// Immutable frame handed to the viewer (shared by `Arc`, never mutated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameSnapshot {
    pub generation: u64,
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}

impl FrameSnapshot {
    pub fn is_empty(&self) -> bool {
        self.rgba.is_empty()
    }
}

// ---------------------------------------------------------------------------
// Rect decoders (stream from the socket, apply in place)
// ---------------------------------------------------------------------------

async fn read_pixel<R: AsyncRead + Unpin>(
    reader: &mut R,
    format: &PixelFormat,
) -> Result<u32, VncError> {
    let mut buf = [0u8; 4];
    let want = format.bytes_per_pixel();
    reader
        .read_exact(&mut buf[..want])
        .await
        .map_err(VncError::io)?;
    format.decode_pixel(&buf[..want])
}

/// Raw rect: `w*h` pixels, row-major.
pub async fn apply_raw_rect<R: AsyncRead + Unpin>(
    fb: &mut Framebuffer,
    format: &PixelFormat,
    header: &RectHeader,
    reader: &mut R,
) -> Result<(), VncError> {
    let bpp = format.bytes_per_pixel();
    let count = header.width as usize * header.height as usize;
    let mut raw = vec![0u8; count * bpp];
    reader.read_exact(&mut raw).await.map_err(VncError::io)?;
    for (i, chunk) in raw.chunks_exact(bpp).enumerate() {
        let pixel = format.decode_pixel(chunk)?;
        let x = header.x + (i % header.width as usize) as u16;
        let y = header.y + (i / header.width as usize) as u16;
        fb.set(x, y, pixel);
    }
    Ok(())
}

/// CopyRect: 4-byte source origin; block copied (overlap-safe).
pub async fn apply_copyrect<R: AsyncRead + Unpin>(
    fb: &mut Framebuffer,
    header: &RectHeader,
    reader: &mut R,
) -> Result<(), VncError> {
    let mut raw = [0u8; 4];
    reader.read_exact(&mut raw).await.map_err(VncError::io)?;
    let src_x = u16::from_be_bytes([raw[0], raw[1]]);
    let src_y = u16::from_be_bytes([raw[2], raw[3]]);
    // Snapshot first: source and destination may overlap (scrolling).
    let mut block = Vec::with_capacity(header.width as usize * header.height as usize);
    for row in 0..header.height {
        for col in 0..header.width {
            block.push(fb.get(src_x + col, src_y + row).unwrap_or(0xFF00_0000));
        }
    }
    for (i, pixel) in block.into_iter().enumerate() {
        let x = header.x + (i % header.width as usize) as u16;
        let y = header.y + (i / header.width as usize) as u16;
        fb.set(x, y, pixel);
    }
    Ok(())
}

/// RRE rect: background fill + subrectangles.
pub async fn apply_rre_rect<R: AsyncRead + Unpin>(
    fb: &mut Framebuffer,
    format: &PixelFormat,
    header: &RectHeader,
    reader: &mut R,
) -> Result<(), VncError> {
    let mut raw = [0u8; 4];
    reader.read_exact(&mut raw).await.map_err(VncError::io)?;
    let count = u32::from_be_bytes(raw);
    let background = read_pixel(reader, format).await?;
    fb.fill_rect(header.x, header.y, header.width, header.height, background);
    for _ in 0..count {
        let pixel = read_pixel(reader, format).await?;
        let mut coords = [0u8; 8];
        reader.read_exact(&mut coords).await.map_err(VncError::io)?;
        let x = header.x + u16::from_be_bytes([coords[0], coords[1]]);
        let y = header.y + u16::from_be_bytes([coords[2], coords[3]]);
        let w = u16::from_be_bytes([coords[4], coords[5]]);
        let h = u16::from_be_bytes([coords[6], coords[7]]);
        fb.fill_rect(x, y, w, h, pixel);
    }
    Ok(())
}

const HEXTILE_RAW: u8 = 0x01;
const HEXTILE_BG: u8 = 0x02;
const HEXTILE_FG: u8 = 0x04;
const HEXTILE_ANY_SUBRECTS: u8 = 0x08;
const HEXTILE_COLOURED: u8 = 0x10;

/// Hextile rect: 16×16 tiles with background/foreground carry-over.
pub async fn apply_hextile_rect<R: AsyncRead + Unpin>(
    fb: &mut Framebuffer,
    format: &PixelFormat,
    header: &RectHeader,
    reader: &mut R,
) -> Result<(), VncError> {
    let mut background = 0xFF00_0000u32;
    let mut foreground = 0xFF00_0000u32;
    let mut tile_y = 0u16;
    while tile_y < header.height {
        let tile_h = (header.height - tile_y).min(16);
        let mut tile_x = 0u16;
        while tile_x < header.width {
            let tile_w = (header.width - tile_x).min(16);
            let mut mask = [0u8; 1];
            reader.read_exact(&mut mask).await.map_err(VncError::io)?;
            let mask = mask[0];
            if mask & HEXTILE_RAW != 0 {
                let count = tile_w as usize * tile_h as usize;
                let bpp = format.bytes_per_pixel();
                let mut raw = vec![0u8; count * bpp];
                reader.read_exact(&mut raw).await.map_err(VncError::io)?;
                for (i, chunk) in raw.chunks_exact(bpp).enumerate() {
                    let pixel = format.decode_pixel(chunk)?;
                    fb.set(
                        header.x + tile_x + (i % tile_w as usize) as u16,
                        header.y + tile_y + (i / tile_w as usize) as u16,
                        pixel,
                    );
                }
                tile_x += 16;
                continue;
            }
            if mask & HEXTILE_BG != 0 {
                background = read_pixel(reader, format).await?;
            }
            if mask & HEXTILE_FG != 0 {
                foreground = read_pixel(reader, format).await?;
            }
            fb.fill_rect(
                header.x + tile_x,
                header.y + tile_y,
                tile_w,
                tile_h,
                background,
            );
            if mask & HEXTILE_ANY_SUBRECTS != 0 {
                let mut count = [0u8; 1];
                reader.read_exact(&mut count).await.map_err(VncError::io)?;
                // A 16×16 tile holds up to 256 single-pixel subrects, which
                // does not fit in one byte: a zero count byte escapes to a
                // uint16 (RFB Hextile rule).
                let total = if count[0] == 0 {
                    let mut wide = [0u8; 2];
                    reader.read_exact(&mut wide).await.map_err(VncError::io)?;
                    u16::from_be_bytes(wide) as usize
                } else {
                    count[0] as usize
                };
                for _ in 0..total {
                    let color = if mask & HEXTILE_COLOURED != 0 {
                        read_pixel(reader, format).await?
                    } else {
                        foreground
                    };
                    let mut xy = [0u8; 1];
                    let mut wh = [0u8; 1];
                    reader.read_exact(&mut xy).await.map_err(VncError::io)?;
                    reader.read_exact(&mut wh).await.map_err(VncError::io)?;
                    let sx = header.x + tile_x + (xy[0] >> 4) as u16;
                    let sy = header.y + tile_y + (xy[0] & 0x0F) as u16;
                    let sw = ((wh[0] >> 4) as u16) + 1;
                    let sh = ((wh[0] & 0x0F) as u16) + 1;
                    fb.fill_rect(sx, sy, sw, sh, color);
                }
            }
            tile_x += 16;
        }
        tile_y += 16;
    }
    Ok(())
}

/// Dispatch one rect by encoding id (unknown ids fail loudly).
pub async fn apply_rect<R: AsyncRead + Unpin>(
    fb: &mut Framebuffer,
    format: &PixelFormat,
    header: &RectHeader,
    reader: &mut R,
) -> Result<(), VncError> {
    match header.encoding {
        ENCODING_RAW => apply_raw_rect(fb, format, header, reader).await,
        ENCODING_COPYRECT => apply_copyrect(fb, header, reader).await,
        ENCODING_RRE => apply_rre_rect(fb, format, header, reader).await,
        ENCODING_HEXTILE => apply_hextile_rect(fb, format, header, reader).await,
        other => Err(VncError::UnsupportedEncoding(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const RED: u32 = 0xFFFF_0000;
    const GREEN: u32 = 0xFF00_FF00;
    const BLUE: u32 = 0xFF00_00FF;

    fn xrgb(pixel: u32) -> [u8; 4] {
        [
            (pixel & 0xFF) as u8,
            ((pixel >> 8) & 0xFF) as u8,
            ((pixel >> 16) & 0xFF) as u8,
            0,
        ]
    }

    fn header(x: u16, y: u16, w: u16, h: u16, encoding: i32) -> RectHeader {
        RectHeader {
            x,
            y,
            width: w,
            height: h,
            encoding,
        }
    }

    #[test]
    fn pixel_format_wire_round_trip() {
        let format = PixelFormat::xrgb32();
        assert_eq!(format.bytes_per_pixel(), 4);
        let wire = format.to_wire();
        assert_eq!(PixelFormat::parse(&wire), format);
        assert_eq!(format.decode_pixel(&xrgb(RED)).unwrap(), RED);
        assert_eq!(format.decode_pixel(&xrgb(GREEN)).unwrap(), GREEN);
    }

    #[test]
    fn rect_header_parses_big_endian() {
        let raw = [0, 1, 0, 2, 0, 3, 0, 4, 0, 0, 0, 5];
        assert_eq!(
            RectHeader::parse(&raw),
            RectHeader {
                x: 1,
                y: 2,
                width: 3,
                height: 4,
                encoding: 5
            }
        );
    }

    #[tokio::test]
    async fn raw_rect_paints_pixels() {
        let format = PixelFormat::xrgb32();
        let mut fb = Framebuffer::new(2, 2);
        let mut payload = Vec::new();
        for pixel in [RED, GREEN, BLUE, RED] {
            payload.extend_from_slice(&xrgb(pixel));
        }
        apply_rect(
            &mut fb,
            &format,
            &header(0, 0, 2, 2, ENCODING_RAW),
            &mut Cursor::new(payload),
        )
        .await
        .unwrap();
        assert_eq!(fb.get(0, 0), Some(RED));
        assert_eq!(fb.get(1, 0), Some(GREEN));
        assert_eq!(fb.get(0, 1), Some(BLUE));
    }

    #[tokio::test]
    async fn copyrect_moves_blocks_overlap_safe() {
        let format = PixelFormat::xrgb32();
        let mut fb = Framebuffer::new(4, 1);
        fb.set(0, 0, RED);
        fb.set(1, 0, GREEN);
        // Copy (0,0,2x1) onto (1,0): overlap must not smear.
        let mut payload = [0, 0, 0, 0];
        payload[0..2].copy_from_slice(&0u16.to_be_bytes());
        apply_rect(
            &mut fb,
            &format,
            &header(1, 0, 2, 1, ENCODING_COPYRECT),
            &mut Cursor::new(payload),
        )
        .await
        .unwrap();
        assert_eq!(fb.get(1, 0), Some(RED));
        assert_eq!(fb.get(2, 0), Some(GREEN));
    }

    #[tokio::test]
    async fn rre_background_plus_subrects() {
        let format = PixelFormat::xrgb32();
        let mut fb = Framebuffer::new(4, 4);
        let mut payload = Vec::new();
        payload.extend_from_slice(&1u32.to_be_bytes());
        payload.extend_from_slice(&xrgb(BLUE));
        payload.extend_from_slice(&xrgb(RED));
        payload.extend_from_slice(&[0, 1, 0, 1, 0, 2, 0, 2]);
        apply_rect(
            &mut fb,
            &format,
            &header(0, 0, 4, 4, ENCODING_RRE),
            &mut Cursor::new(payload),
        )
        .await
        .unwrap();
        assert_eq!(fb.get(0, 0), Some(BLUE));
        assert_eq!(fb.get(1, 1), Some(RED));
        assert_eq!(fb.get(2, 2), Some(RED));
        assert_eq!(fb.get(3, 3), Some(BLUE));
    }

    #[tokio::test]
    async fn hextile_raw_tile_and_subrects() {
        let format = PixelFormat::xrgb32();
        let mut fb = Framebuffer::new(20, 4);
        let mut payload = Vec::new();
        // Tile (0,0): raw 16x4.
        payload.push(HEXTILE_RAW);
        for _ in 0..16 * 4 {
            payload.extend_from_slice(&xrgb(GREEN));
        }
        // Tile (16,0): bg blue, fg red + one 2x2 subrect at (1,1).
        payload.push(HEXTILE_BG | HEXTILE_FG | HEXTILE_ANY_SUBRECTS);
        payload.extend_from_slice(&xrgb(BLUE));
        payload.extend_from_slice(&xrgb(RED));
        payload.push(1);
        payload.push(0x11);
        payload.push(0x11);
        apply_rect(
            &mut fb,
            &format,
            &header(0, 0, 20, 4, ENCODING_HEXTILE),
            &mut Cursor::new(payload),
        )
        .await
        .unwrap();
        assert_eq!(fb.get(0, 0), Some(GREEN));
        assert_eq!(fb.get(16, 0), Some(BLUE));
        assert_eq!(fb.get(17, 1), Some(RED));
        assert_eq!(fb.get(18, 2), Some(RED));
        assert_eq!(fb.get(19, 3), Some(BLUE));
    }

    #[tokio::test]
    async fn unknown_encoding_fails_loudly() {
        let format = PixelFormat::xrgb32();
        let mut fb = Framebuffer::new(2, 2);
        let err = apply_rect(
            &mut fb,
            &format,
            &header(0, 0, 2, 2, -260),
            &mut Cursor::new(Vec::new()),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, VncError::UnsupportedEncoding(-260)));
    }

    #[test]
    fn snapshot_exports_rgba_for_wgpu() {
        let mut fb = Framebuffer::new(1, 1);
        fb.set(0, 0, RED);
        let snapshot = fb.snapshot(7);
        assert_eq!(snapshot.rgba, vec![255, 0, 0, 255]);
        assert_eq!(snapshot.generation, 7);
    }
}
