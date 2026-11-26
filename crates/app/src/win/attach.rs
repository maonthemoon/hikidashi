//! 書く窓(K22)に添付する画像のファイルを読む. Windows標準のWICで画素にし, クリップボードに入れる形(CF_DIB)を作る.
//! PNGのファイルは, そのバイト列もPNGの形式として持つ(ブラウザーは貼るときにPNGを先に読む).

use hikidashi_core::attach::{self, Image};

/// 読むファイルの大きさの上限(バイト)
const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// 画素の数の上限(32bitの画素で200MBほど)
const MAX_PIXELS: u64 = 50_000_000;

/// 画像のファイルを添付の形にする. 読めなければ, 知らせに出す文を返す
pub fn load_file(path: &str) -> Result<Image, String> {
    let name = path.rsplit('\\').next().unwrap_or(path);
    let size = std::fs::metadata(path).map_err(|_| format!("Couldn't read {name}"))?.len();
    if size > MAX_FILE_BYTES {
        return Err(format!("{name} is too large to attach"));
    }
    let bytes = std::fs::read(path).map_err(|_| format!("Couldn't read {name}"))?;
    let (width, height, px) = decode(&bytes).ok_or_else(|| format!("Couldn't read {name} as an image"))?;
    let dib = attach::dib_from_bgra(width, height, &px).ok_or_else(|| format!("Couldn't read {name} as an image"))?;
    let png = attach::png_size(&bytes).is_some().then_some(bytes);
    Ok(Image { width, height, dib: Some(dib), png })
}

/// 画像のバイト列を, 上の行から並んだ32bitの画素(B, G, R, A)にする. GIFは最初のコマ
fn decode(bytes: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    use windows::Win32::Graphics::Imaging::*;
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    unsafe {
        let f: IWICImagingFactory = CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let stream = f.CreateStream().ok()?;
        stream.InitializeFromMemory(bytes).ok()?;
        let dec = f.CreateDecoderFromStream(&stream, std::ptr::null(), WICDecodeMetadataCacheOnDemand).ok()?;
        let frame = dec.GetFrame(0).ok()?;
        let (mut w, mut h) = (0u32, 0u32);
        frame.GetSize(&mut w, &mut h).ok()?;
        if w == 0 || h == 0 || w as u64 * h as u64 > MAX_PIXELS {
            return None;
        }
        let conv = f.CreateFormatConverter().ok()?;
        conv.Initialize(&frame, &GUID_WICPixelFormat32bppBGRA, WICBitmapDitherTypeNone, None, 0.0, WICBitmapPaletteTypeCustom).ok()?;
        let stride = w * 4;
        let mut px = vec![0u8; stride as usize * h as usize];
        conv.CopyPixels(std::ptr::null(), stride, &mut px).ok()?;
        Some((w, h, px))
    }
}
