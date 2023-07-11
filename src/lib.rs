//! About the smallest drawing API you could ask for
use std::{
    alloc::{alloc, dealloc, handle_alloc_error, Layout},
    ffi::CString,
};
use x11::{
    xft::{
        FcPattern, XftColor, XftColorAllocName, XftDrawCreate, XftFont, XftFontClose,
        XftFontOpenName, XftNameParse,
    },
    xlib::{
        CapButt, Display, Drawable, False, JoinMiter, LineSolid, Window, XCopyArea, XCreateGC,
        XCreatePixmap, XDefaultColormap, XDefaultDepth, XDefaultVisual, XDrawRectangle,
        XFillRectangle, XFreeGC, XFreePixmap, XOpenDisplay, XSetForeground, XSetLineAttributes,
        XSync, GC,
    },
};

const SCREEN: i32 = 0;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("The provided color string contained an internal null byte")]
    InvalidColorString,

    #[error("The provided font name contained an internal null byte")]
    InvalidFontName,

    #[error("Unable to allocate the requested color using Xft")]
    UnableToAllocateColor,

    #[error("Unable to open '{0}' as a font using Xft")]
    UnableToOpenFont(String),

    #[error("Unable to parse '{0}' as an Xft font patten")]
    UnableToParseFontPattern(String),

    #[error("'{0}' is not a registered colorscheme")]
    UnknownColorscheme(String),
}

type Result<T> = std::result::Result<T, Error>;

// Fonts contain a resource that requires a Display to free on Drop so they
// are owned by their parent Draw and cleaned up when the Draw is dropped
struct Font {
    h: i32,
    xfont: *mut XftFont,
    pattern: *mut FcPattern,
}

struct ColorScheme {
    name: String,
    fg: *mut XftColor,
    bg: *mut XftColor,
}

impl ColorScheme {
    unsafe fn fg(&self) -> u64 {
        (*self.fg).pixel
    }

    unsafe fn bg(&self) -> u64 {
        (*self.bg).pixel
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
        self.free_fonts();

        let mut fonts = Vec::with_capacity(font_names.len());
        for name in font_names {
            fonts.push(self.font_from_name(name)?);
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

    fn free_fonts(&mut self) {
        unsafe {
            for f in self.fonts.drain(0..) {
                XftFontClose(self.dpy, f.xfont);
            }
        }
    }

    fn font_from_name(&mut self, name: &str) -> Result<Font> {
        let (xfont, pattern, h) = unsafe {
            let c_name = CString::new(name).map_err(|_| Error::InvalidFontName)?;
            let xfont = XftFontOpenName(self.dpy, SCREEN, c_name.as_ptr());
            if xfont.is_null() {
                return Err(Error::UnableToOpenFont(name.to_string()));
            }

            let pattern = XftNameParse(c_name.as_ptr());
            if pattern.is_null() {
                XftFontClose(self.dpy, xfont);
                return Err(Error::UnableToParseFontPattern(name.to_string()));
            }

            let h = (*xfont).ascent + (*xfont).descent;

            (xfont, pattern, h)
        };

        Ok(Font { xfont, pattern, h })
    }

    // TODO: should accept impl Into<penrose::Color>
    pub fn add_colorscheme(&mut self, name: &str, fg: &str, bg: &str) -> Result<()> {
        let cs = ColorScheme {
            name: name.to_string(),
            fg: self.color_from_name(fg)?,
            bg: self.color_from_name(bg)?,
        };
        self.schemes.push(cs);

        Ok(())
    }

    fn color_from_name(&mut self, color: &str) -> Result<*mut XftColor> {
        unsafe {
            // https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html#tymethod.alloc
            let layout = Layout::new::<XftColor>();
            let ptr = alloc(layout);
            if ptr.is_null() {
                handle_alloc_error(layout);
            }

            let c_name = CString::new(color).map_err(|_| Error::InvalidColorString)?;
            let res = XftColorAllocName(
                self.dpy,
                XDefaultVisual(self.dpy, SCREEN),
                XDefaultColormap(self.dpy, SCREEN),
                c_name.as_ptr(),
                ptr as *mut XftColor,
            );

            if res == 0 {
                Err(Error::UnableToAllocateColor)
            } else {
                Ok(ptr as *mut XftColor)
            }
        }
    }

    fn free_colors(&mut self) {
        unsafe {
            let layout = Layout::new::<XftColor>();

            for ColorScheme { fg, bg, .. } in self.schemes.drain(0..) {
                for ptr in [fg, bg] {
                    // TODO: check if this should be done use XftFreeColor
                    dealloc(ptr as *mut u8, layout);
                }
            }
        }
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

    pub fn draw_text(&mut self, txt: &str, lpad: u32, r: Rect, invert: bool) -> Result<()> {
        self.fill_rect(r, !invert)?; // !invert so we get the other color

        let d = unsafe {
            XftDrawCreate(
                self.dpy,
                self.drawable,
                XDefaultVisual(self.dpy, SCREEN),
                XDefaultColormap(self.dpy, SCREEN),
            )
        };

        let Rect { mut x, y, mut w, h } = r;
        w += lpad;
        x -= lpad as i32;

        todo!("deal with strings and font rendering in C");

        Ok(())
    }

    pub fn flush_to(&mut self, win: u32, Rect { x, y, w, h }: Rect) {
        let win = win as Window;

        unsafe {
            XCopyArea(self.dpy, self.drawable, win, self.gc, x, y, w, h, x, y);
            XSync(self.dpy, False);
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
        }
    }
}

// unsigned int drw_fontset_getwidth(Drw *drw, const char *text);
// void drw_font_getexts(Fnt *font, const char *text, unsigned int len, unsigned int *w, unsigned int *h);
