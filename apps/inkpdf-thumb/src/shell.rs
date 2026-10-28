//! The COM side: the class factory, the two interfaces the shell uses, and the DLL's exports.

use std::ffi::{OsString, c_void};
use std::os::windows::ffi::OsStringExt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::Mutex;

use windows::Win32::Foundation::{CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, E_INVALIDARG, E_POINTER, S_FALSE};
use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, HBITMAP};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl, IStream};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT, GetModuleFileNameW, GetModuleHandleExW,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IInitializeWithStream, IInitializeWithStream_Impl};
use windows::Win32::UI::Shell::{IThumbnailProvider, IThumbnailProvider_Impl, WTS_ALPHATYPE, WTSAT_RGB};
use windows::core::{BOOL, GUID, HRESULT, IUnknown, Interface, PCWSTR, Ref, Result, implement};

use crate::client::{self, Picture};

/// The class id the installer registers for this handler.
pub const CLSID_INKPDF_THUMBNAIL: GUID = GUID::from_u128(0x7a4c2e19_5b3d_4f80_a6d1_92c0e8b54f37);

/// One thumbnail request: the shell hands over the file's stream, then asks for a bitmap.
#[implement(IInitializeWithStream, IThumbnailProvider)]
struct Thumbnail {
    pdf: Mutex<Option<Vec<u8>>>,
}

impl IInitializeWithStream_Impl for Thumbnail_Impl {
    fn Initialize(&self, stream: Ref<'_, IStream>, _mode: u32) -> Result<()> {
        let stream = stream.ok()?;
        let bytes = read_all(stream)?;
        *self.pdf.lock().map_err(|_| E_FAIL)? = Some(bytes);
        Ok(())
    }
}

impl IThumbnailProvider_Impl for Thumbnail_Impl {
    fn GetThumbnail(&self, cx: u32, phbmp: *mut HBITMAP, pdwalpha: *mut WTS_ALPHATYPE) -> Result<()> {
        if phbmp.is_null() || pdwalpha.is_null() {
            return Err(E_POINTER.into());
        }
        let pdf = self.pdf.lock().map_err(|_| E_FAIL)?.take().ok_or(E_FAIL)?;
        // The helper draws it; a broken PDF can only hurt that helper, and anything else going wrong
        // here is caught too, so it never reaches the program showing thumbnails.
        let helper = helper_path().ok_or(E_FAIL)?;
        let thumbnail = catch_unwind(AssertUnwindSafe(|| client::render(&helper, pdf, cx))).ok().flatten().ok_or(E_FAIL)?;
        let bitmap = make_bitmap(&thumbnail)?;
        // SAFETY: both pointers were checked above, and the shell gave them to us to fill in.
        unsafe {
            *phbmp = bitmap;
            *pdwalpha = WTSAT_RGB;
        }
        Ok(())
    }
}

/// Files larger than this are not read for a thumbnail: the shell asks for many at once.
const MAX_PDF_BYTES: usize = 256 << 20;

/// Where the helper is: inkpdf.exe beside this DLL.
fn helper_path() -> Option<PathBuf> {
    let mut module = windows::Win32::Foundation::HMODULE::default();
    let anchor: extern "system" fn() -> HRESULT = DllCanUnloadNow;
    // SAFETY: the address is this DLL's own exported function, and the handle's reference count is
    // left alone, so nothing needs releasing.
    unsafe {
        GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            PCWSTR(anchor as *const u16),
            &mut module,
        )
        .ok()?;
    }
    let mut buffer = [0u16; 1024];
    // SAFETY: the buffer is valid for its whole length.
    let length = unsafe { GetModuleFileNameW(Some(module), &mut buffer) } as usize;
    (length > 0 && length < buffer.len()).then(|| PathBuf::from(OsString::from_wide(&buffer[..length])).with_file_name("inkpdf.exe"))
}

/// Everything the stream holds, up to the size limit.
fn read_all(stream: &IStream) -> Result<Vec<u8>> {
    const CHUNK: usize = 1 << 20;
    let mut out = Vec::new();
    loop {
        let start = out.len();
        out.resize(start + CHUNK, 0);
        let mut read = 0u32;
        // SAFETY: the buffer has room for CHUNK bytes at `start`, and `read` outlives the call.
        let result = unsafe { stream.Read(out[start..].as_mut_ptr().cast(), CHUNK as u32, Some(&mut read)) };
        out.truncate(start + read as usize);
        result.ok()?;
        if read == 0 {
            return Ok(out);
        }
        if out.len() > MAX_PDF_BYTES {
            return Err(E_FAIL.into());
        }
    }
}

/// A 32-bit device-independent bitmap holding the thumbnail's pixels.
fn make_bitmap(t: &Picture) -> Result<HBITMAP> {
    let (width, height) = (i32::try_from(t.width).map_err(|_| E_INVALIDARG)?, i32::try_from(t.height).map_err(|_| E_INVALIDARG)?);
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: rows run from the top, as ours do.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bits: *mut c_void = std::ptr::null_mut();
    // SAFETY: `info` describes a 32-bit top-down bitmap and `bits` receives the pixel memory.
    let bitmap = unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)? };
    if bits.is_null() || t.bgra.len() != t.width as usize * t.height as usize * 4 {
        return Err(E_FAIL.into());
    }
    // SAFETY: the DIB section is exactly width * height * 4 bytes, which is what is copied.
    unsafe { std::ptr::copy_nonoverlapping(t.bgra.as_ptr(), bits.cast::<u8>(), t.bgra.len()) };
    Ok(bitmap)
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Ref<'_, IUnknown>, iid: *const GUID, object: *mut *mut c_void) -> Result<()> {
        if object.is_null() || iid.is_null() {
            return Err(E_POINTER.into());
        }
        // SAFETY: checked above; the shell gave us the slot to fill.
        unsafe { *object = std::ptr::null_mut() };
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let instance: IUnknown = Thumbnail { pdf: Mutex::new(None) }.into();
        // SAFETY: `iid` and `object` are the shell's, valid for this call.
        unsafe { instance.query(iid, object).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

/// Where the shell asks for this handler's class factory.
///
/// # Safety
/// Called by COM with valid pointers.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(class: *const GUID, iid: *const GUID, object: *mut *mut c_void) -> HRESULT {
    if class.is_null() || iid.is_null() || object.is_null() {
        return E_POINTER;
    }
    // SAFETY: checked above.
    unsafe {
        *object = std::ptr::null_mut();
        if *class != CLSID_INKPDF_THUMBNAIL {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
    }
    let factory: IClassFactory = Factory.into();
    // SAFETY: `iid` and `object` are COM's, valid for this call.
    unsafe { factory.query(iid, object) }
}

/// The DLL stays loaded: it is small, and the shell loads it again and again.
#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}
