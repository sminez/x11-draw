//! About the smallest drawing API you could ask for
use fontconfig_sys::{
    constants::{FC_CHARSET, FC_SCALABLE},
    FcCharSetAddChar, FcCharSetCreate, FcCharSetDestroy, FcConfig, FcConfigSubstitute,
    FcDefaultSubstitute, FcMatchPattern, FcPatternAddBool, FcPatternAddCharSet, FcPatternDestroy,
    FcPatternDuplicate,
};
use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    ffi::{CStr, CString},
};
use x11::{
    xft::{
        FcPattern, FcResult, XftCharExists, XftColor, XftColorAllocName, XftDrawCreate,
        XftDrawStringUtf8, XftFont, XftFontClose, XftFontMatch, XftFontOpenName,
        XftFontOpenPattern, XftNameParse, XftNameUnparse, XftTextExtentsUtf8,
    },
    xlib::{
        CapButt, Display, Drawable, False, JoinMiter, LineSolid, Window, XCloseDisplay, XCopyArea,
        XCreateGC, XCreatePixmap, XDefaultColormap, XDefaultDepth, XDefaultVisual, XDrawRectangle,
        XFillRectangle, XFreeGC, XFreePixmap, XOpenDisplay, XSetForeground, XSetLineAttributes,
        XSync, GC,
    },
    xrender::XGlyphInfo,
};

const SCREEN: i32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The provided color string contained an internal null byte")]
    InvalidColorString,

    #[error("The provided font name contained an internal null byte")]
    InvalidFontName,

    #[error("Unable to find a fallback font for '{0}'")]
    NoFallbackFontForChar(char),

    #[error("Unable to allocate the requested color using Xft")]
    UnableToAllocateColor,

    #[error("Unable to open '{0}' as a font using Xft")]
    UnableToOpenFont(String),

    #[error("Unable to open font from FcPattern using Xft")]
    UnableToOpenFontPattern,

    #[error("Unable to parse '{0}' as an Xft font patten")]
    UnableToParseFontPattern(String),

    #[error("'{0}' is not a registered colorscheme")]
    UnknownColorscheme(String),
}

type Result<T> = std::result::Result<T, Error>;

// Fonts contain a resource that requires a Display to free on Drop so they
// are owned by their parent Draw and cleaned up when the Draw is dropped
//
// https://man.archlinux.org/man/extra/libxft/XftFontMatch.3.en
// https://refspecs.linuxfoundation.org/fontconfig-2.6.0/index.html
struct Font {
    name: String,
    h: i32,
    xfont: *mut XftFont,
    pattern: Option<*mut FcPattern>,
}

impl Font {
    fn try_new_from_name(dpy: *mut Display, name: &str) -> Result<Self> {
        let (xfont, pattern, h) = unsafe {
            let c_name = CString::new(name).map_err(|_| Error::InvalidFontName)?;
            let xfont = XftFontOpenName(dpy, SCREEN, c_name.as_ptr());
            if xfont.is_null() {
                return Err(Error::UnableToOpenFont(name.to_string()));
            }

            let pattern = XftNameParse(c_name.as_ptr());
            if pattern.is_null() {
                XftFontClose(dpy, xfont);
                return Err(Error::UnableToParseFontPattern(name.to_string()));
            }

            let h = (*xfont).ascent + (*xfont).descent;

            (xfont, Some(pattern), h)
        };

        Ok(Font {
            name: name.to_string(),
            xfont,
            pattern,
            h,
        })
    }

    fn try_new_from_pattern(dpy: *mut Display, pat: *mut FcPattern) -> Result<Self> {
        let (name, xfont, h) = unsafe {
            let xfont = XftFontOpenPattern(dpy, pat);
            if xfont.is_null() {
                return Err(Error::UnableToOpenFontPattern);
            }

            let h = (*xfont).ascent + (*xfont).descent;

            let buffer = CString::new(vec![b' '; 1024]).unwrap();
            XftNameUnparse(pat, buffer.as_ptr() as *mut _, 1024);
            let name = buffer.into_string().expect("valid utf8");

            (name, xfont, h)
        };

        Ok(Font {
            name,
            xfont,
            pattern: None,
            h,
        })
    }

    fn renderable_prefix<'a>(&self, dpy: *mut Display, txt: &'a str) -> (&'a str, &'a str) {
        if let Some(ix) = txt.find(|c| !self.contains_char(dpy, c)) {
            txt.split_at(ix)
        } else {
            (txt, "")
        }
    }

    fn contains_char(&self, dpy: *mut Display, c: char) -> bool {
        unsafe { XftCharExists(dpy, self.xfont, c as u32) == 1 }
    }

    fn get_exts(&self, dpy: *mut Display, txt: &str) -> (u32, u32) {
        unsafe {
            // https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#tymethod.alloc
            let layout = Layout::new::<XGlyphInfo>();
            let ptr = alloc(layout);
            if ptr.is_null() {
                handle_alloc_error(layout);
            }
            let ext = ptr as *mut XGlyphInfo;

            let c_str = CString::new(txt).unwrap();
            XftTextExtentsUtf8(
                dpy,
                self.xfont,
                c_str.as_ptr() as *mut u8,
                c_str.as_bytes().len() as i32,
                ext,
            );

            ((*ext).xOff as u32, self.h as u32)
        }
    }

    /// Find a font that can handle a given character using fontconfig and this font's pattern
    fn new_for_char(&self, dpy: *mut Display, c: char) -> Result<Self> {
        let pat = self.fc_font_match(dpy, c)?;

        Err(Error::NoFallbackFontForChar(c))
    }

    fn fc_font_match(&self, dpy: *mut Display, c: char) -> Result<*mut FcPattern> {
        unsafe {
            let charset = FcCharSetCreate();
            FcCharSetAddChar(charset, c as u32);

            let pat = FcPatternDuplicate(self.pattern.unwrap() as *const _);
            FcPatternAddCharSet(pat, FC_CHARSET.as_ptr(), charset);
            FcPatternAddBool(pat, FC_SCALABLE.as_ptr(), 1); // FcTrue=1

            FcConfigSubstitute(std::ptr::null::<FcConfig>() as *mut _, pat, FcMatchPattern);
            FcDefaultSubstitute(pat);

            // https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#tymethod.alloc
            let layout = Layout::new::<FcResult>();
            let ptr = alloc(layout);
            if ptr.is_null() {
                handle_alloc_error(layout);
            }
            let res = ptr as *mut FcResult;

            // Passing the pointer from fontconfig_sys to x11 here
            let font_match = XftFontMatch(dpy, SCREEN, pat as *const _, res);

            FcCharSetDestroy(charset);
            FcPatternDestroy(pat);

            if font_match.is_null() {
                Err(Error::NoFallbackFontForChar(c))
            } else {
                Ok(font_match as *mut _)
            }
        }
    }
}

struct ColorScheme {
    name: String,
    fg: *mut XftColor,
    bg: *mut XftColor,
}

impl ColorScheme {
    // TODO: should accept impl Into<penrose::Color>
    fn try_new(dpy: *mut Display, name: &str, fg: &str, bg: &str) -> Result<Self> {
        let (fg, bg) = unsafe {
            (
                try_xftcolor_from_name(dpy, fg)?,
                try_xftcolor_from_name(dpy, bg)?,
            )
        };

        Ok(ColorScheme {
            name: name.to_string(),
            fg,
            bg,
        })
    }

    unsafe fn fg(&self) -> u64 {
        (*self.fg).pixel
    }

    unsafe fn bg(&self) -> u64 {
        (*self.bg).pixel
    }
}

unsafe fn try_xftcolor_from_name(dpy: *mut Display, color: &str) -> Result<*mut XftColor> {
    // https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#tymethod.alloc
    let layout = Layout::new::<XftColor>();
    let ptr = alloc(layout);
    if ptr.is_null() {
        handle_alloc_error(layout);
    }

    let c_name = CString::new(color).map_err(|_| Error::InvalidColorString)?;
    let res = XftColorAllocName(
        dpy,
        XDefaultVisual(dpy, SCREEN),
        XDefaultColormap(dpy, SCREEN),
        c_name.as_ptr(),
        ptr as *mut XftColor,
    );

    if res == 0 {
        Err(Error::UnableToAllocateColor)
    } else {
        Ok(ptr as *mut XftColor)
    }
}

#[derive(Debug, Copy, Clone)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

pub struct Draw {
    dpy: *mut Display,
    root: Window,
    drawable: Drawable,
    gc: GC,
    schemes: Vec<ColorScheme>,
    fonts: Vec<Font>,
}

impl Draw {
    pub fn new(root: u32, w: u32, h: u32) -> Self {
        let root = root as Window;
        let (dpy, drawable, gc) = unsafe {
            let dpy = XOpenDisplay(std::ptr::null());
            let depth = XDefaultDepth(dpy, SCREEN) as u32;
            let drawable = XCreatePixmap(dpy, root, w, h, depth);
            let gc = XCreateGC(dpy, root, 0, std::ptr::null_mut());
            XSetLineAttributes(dpy, gc, 1, LineSolid, CapButt, JoinMiter);

            (dpy, drawable, gc)
        };

        Self {
            dpy,
            root,
            drawable,
            gc,
            schemes: Vec::new(),
            fonts: Vec::new(),
        }
    }

    pub fn resize(&mut self, w: u32, h: u32) {
        unsafe {
            if self.drawable != 0 {
                XFreePixmap(self.dpy, self.drawable);
            }

            let depth = XDefaultDepth(self.dpy, SCREEN) as u32;
            self.drawable = XCreatePixmap(self.dpy, self.root, w, h, depth);
        }
    }

    pub fn set_fonts(&mut self, font_names: &[&str]) -> Result<()> {
        unsafe {
            self.free_fonts();
        }

        let mut fonts = Vec::with_capacity(font_names.len());
        for name in font_names {
            fonts.push(Font::try_new_from_name(self.dpy, name)?);
        }

        self.fonts = fonts;

        Ok(())
    }

    pub fn set_colorscheme(&mut self, scheme: &str) -> Result<()> {
        let ix = self
            .schemes
            .iter()
            .enumerate()
            .find(|(_, s)| s.name == scheme)
            .map(|(i, _)| i)
            .ok_or_else(|| Error::UnknownColorscheme(scheme.to_string()))?;

        if ix != 0 {
            self.schemes.swap(0, ix);
        }

        Ok(())
    }

    pub fn add_colorscheme(&mut self, name: &str, fg: &str, bg: &str) -> Result<()> {
        let cs = ColorScheme::try_new(self.dpy, name, fg, bg)?;
        self.schemes.push(cs);

        Ok(())
    }

    pub fn draw_rect(&mut self, Rect { x, y, w, h }: Rect, inverted: bool) -> Result<()> {
        let scheme = &self.schemes[0];

        unsafe {
            let pixel = if inverted { scheme.bg() } else { scheme.fg() };
            XSetForeground(self.dpy, self.gc, pixel);
            XDrawRectangle(self.dpy, self.drawable, self.gc, x, y, w, h);
        }

        Ok(())
    }

    pub fn fill_rect(&mut self, Rect { x, y, w, h }: Rect, invert: bool) -> Result<()> {
        let scheme = &self.schemes[0];

        unsafe {
            let pixel = if invert { scheme.bg() } else { scheme.fg() };
            XSetForeground(self.dpy, self.gc, pixel);
            XFillRectangle(self.dpy, self.drawable, self.gc, x, y, w, h);
        }

        Ok(())
    }

    // https://keithp.com/~keithp/talks/xtc2001/xft.pdf
    // https://keithp.com/~keithp/render/Xft.tutorial
    pub fn draw_text(&mut self, txt: &str, lpad: u32, r: Rect, invert: bool) -> Result<()> {
        self.fill_rect(r, !invert)?; // !invert so we get the other color

        unsafe {
            let d = XftDrawCreate(
                self.dpy,
                self.drawable,
                XDefaultVisual(self.dpy, SCREEN),
                XDefaultColormap(self.dpy, SCREEN),
            );

            let scheme = &self.schemes[0];
            let color = if invert { scheme.bg } else { scheme.fg };
            let Rect { mut x, mut y, .. } = r;
            // w -= lpad;
            x += lpad as i32;
            y += lpad as i32;
            let c_str = CString::new(txt).unwrap();
            XftDrawStringUtf8(
                d,
                color,
                self.fonts[0].xfont,
                x,
                y,
                c_str.as_ptr() as *mut u8,
                c_str.as_bytes().len() as i32,
            );
        }

        Ok(())
    }

    pub fn show_font_match_for_chars(&mut self, txt: &str) {
        // Prioritise fonts based on their ordering
        // -> when primary is active we are greedy
        // -> when fallback is active we match single chars
        // -> might want/need to look at https://crates.io/crates/fontconfig for fallback
    }

    pub fn flush_to(&mut self, win: u32, Rect { x, y, w, h }: Rect) {
        let win = win as Window;

        unsafe {
            XCopyArea(self.dpy, self.drawable, win, self.gc, x, y, w, h, x, y);
            XSync(self.dpy, False);
        }
    }

    unsafe fn free_fonts(&mut self) {
        for f in self.fonts.drain(0..) {
            XftFontClose(self.dpy, f.xfont);
        }
    }

    unsafe fn free_colors(&mut self) {
        let layout = Layout::new::<XftColor>();

        for ColorScheme { fg, bg, .. } in self.schemes.drain(0..) {
            for ptr in [fg, bg] {
                // TODO: check if this should be done use XftFreeColor
                dealloc(ptr as *mut u8, layout);
            }
        }
    }
}

impl Drop for Draw {
    fn drop(&mut self) {
        unsafe {
            XFreePixmap(self.dpy, self.drawable);
            XFreeGC(self.dpy, self.gc);
            self.free_colors();
            self.free_fonts();
            XCloseDisplay(self.dpy);
        }
    }
}

// unsigned int drw_fontset_getwidth(Drw *drw, const char *text);
// void drw_font_getexts(Fnt *font, const char *text, unsigned int len, unsigned int *w, unsigned int *h);
