use std::ffi::c_void;
use std::path::Path;
use std::sync::Arc;

use gpui_kit::RenderImage;
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, HBITMAP, ReleaseDC,
};
use windows::Win32::UI::Shell::{
    IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY,
};
use windows::core::{HSTRING, Interface};

/// Caller thread must have COM initialized.
pub fn from_shell_item(item: &IShellItem, size: i32) -> Option<Arc<RenderImage>> {
    let factory: IShellItemImageFactory = item.cast().ok()?;
    let hbitmap = unsafe { factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY) }.ok()?;
    let pixels = read_bgra(hbitmap);
    let _ = unsafe { DeleteObject(hbitmap.into()) };
    let (width, height, buf) = pixels?;
    let frame = image::Frame::new(image::RgbaImage::from_raw(width, height, buf)?);
    Some(Arc::new(RenderImage::new(vec![frame])))
}

/// Caller thread must have COM initialized.
pub fn for_path(path: &Path, size: i32) -> Option<Arc<RenderImage>> {
    let item: IShellItem =
        unsafe { SHCreateItemFromParsingName(&HSTRING::from(path), None) }.ok()?;
    from_shell_item(&item, size)
}

fn read_bgra(hbitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let mut bm = BITMAP::default();
    let got = unsafe {
        GetObjectW(
            hbitmap.into(),
            size_of::<BITMAP>() as i32,
            Some((&raw mut bm).cast::<c_void>()),
        )
    };
    if got == 0 || bm.bmWidth <= 0 || bm.bmHeight == 0 {
        return None;
    }
    let (width, height) = (bm.bmWidth, bm.bmHeight.abs());
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative height requests top-down rows.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; width as usize * height as usize * 4];
    let hdc = unsafe { GetDC(None) };
    if hdc.is_invalid() {
        return None;
    }
    let lines = unsafe {
        GetDIBits(
            hdc,
            hbitmap,
            0,
            height as u32,
            Some(buf.as_mut_ptr().cast::<c_void>()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };
    unsafe { ReleaseDC(None, hdc) };
    if lines != height {
        return None;
    }
    to_straight_alpha(&mut buf);
    Some((width as u32, height as u32, buf))
}

// Shell bitmaps are premultiplied; gpui's RenderImage expects straight-alpha BGRA.
fn to_straight_alpha(buf: &mut [u8]) {
    if buf.chunks_exact(4).all(|px| px[3] == 0) {
        // Bitmap without an alpha channel: treat as fully opaque.
        buf.chunks_exact_mut(4).for_each(|px| px[3] = 255);
        return;
    }
    for px in buf.chunks_exact_mut(4) {
        let a = px[3] as u32;
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}
