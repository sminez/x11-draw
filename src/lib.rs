//! About the smallest drawing API you could ask for
use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    ffi::{CString, NulError},
};
use x11::{
    xft::{XftColor, XftColorAllocName, XftDrawCreate, XftDrawStringUtf8},
    xlib::{
        CapButt, Display, Drawable, False, JoinMiter, LineSolid, Window, XCopyArea, XCreateGC,
        XCreatePixmap, XDefaultColormap, XDefaultDepth, XDefaultVisual, XDrawRectangle,
        XFillRectangle, XFreeGC, XFreePixmap, XOpenDisplay, XSetForeground, XSetLineAttributes,
        XSync, GC,
    },
};

mod fontset;
use fontset::Fontset;

pub(crate) const SCREEN: i32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Unable to find a fallback font for '{0}'")]
    NoFallbackFontForChar(char),

    #[error(transparent)]
    NulError(#[from] NulError),

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

#[derive(Debug)]
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

    let c_name = CString::new(color)?;
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

// TODO: just use the penrose Rect struct once this is moved over
#[derive(Debug, Copy, Clone)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug)]
pub struct Draw {
    dpy: *mut Display,
    root: Window,
    drawable: Drawable,
    gc: GC,
    fs: Fontset,
    schemes: Vec<ColorScheme>,
}

impl Draw {
    pub fn new(root: u32, w: u32, h: u32, fnt: &str) -> Result<Self> {
        let root = root as Window;
        let (dpy, drawable, gc) = unsafe {
            let dpy = XOpenDisplay(std::ptr::null());
            let depth = XDefaultDepth(dpy, SCREEN) as u32;
            let drawable = XCreatePixmap(dpy, root, w, h, depth);
            let gc = XCreateGC(dpy, root, 0, std::ptr::null_mut());
            XSetLineAttributes(dpy, gc, 1, LineSolid, CapButt, JoinMiter);

            (dpy, drawable, gc)
        };

        Ok(Self {
            dpy,
            root,
            drawable,
            gc,
            fs: Fontset::try_new(dpy, fnt)?,
            schemes: Vec::new(),
        })
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

    pub fn set_font(&mut self, font_name: &str) -> Result<()> {
        self.fs = Fontset::try_new(self.dpy, font_name)?;

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

    pub fn show_font_match_for_chars(&mut self, txt: &str) {
        for (chunk, fm) in self.fs.per_font_chunks(txt) {
            let ext = self.fs.fnt(fm).get_exts(self.dpy, chunk);
            println!("{fm:?} [extent: {ext:?}] -> '{chunk}'");
        }
    }

    // TODO: Need to bounds checks
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
            let Rect { mut x, y, h, .. } = r;
            x += lpad as i32;

            for (chunk, fm) in self.fs.per_font_chunks(txt).into_iter() {
                let fnt = self.fs.fnt(fm);
                let (chunk_w, chunk_h) = fnt.get_exts(self.dpy, chunk)?;
                let chunk_y = y + (h as i32 - chunk_h) / 2 + (*fnt.xfont).ascent;

                let c_str = CString::new(chunk).unwrap();
                XftDrawStringUtf8(
                    d,
                    color,
                    self.fs.fnt(fm).xfont,
                    x,
                    chunk_y,
                    c_str.as_ptr() as *mut _,
                    c_str.as_bytes().len() as i32,
                );

                x += chunk_w;
            }
        }

        Ok(())
    }

    pub fn text_extent(&mut self, txt: &str) -> Result<(i32, i32)> {
        let (mut w, mut h) = (0, 0);
        for (chunk, fm) in self.fs.per_font_chunks(txt) {
            let (cw, ch) = self.fs.fnt(fm).get_exts(self.dpy, chunk)?;
            w += cw;
            h += ch;
        }

        Ok((w, h))
    }

    pub fn flush_to(&mut self, win: u32, Rect { x, y, w, h }: Rect) {
        let win = win as Window;

        unsafe {
            XCopyArea(self.dpy, self.drawable, win, self.gc, x, y, w, h, x, y);
            XSync(self.dpy, False);
        }
    }

    unsafe fn free_colors(&mut self) {
        let layout = Layout::new::<XftColor>();

        for ColorScheme { fg, bg, .. } in self.schemes.drain(0..) {
            for ptr in [fg, bg] {
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
        }
    }
}
