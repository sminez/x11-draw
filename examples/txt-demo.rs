//! Demo of the text rendering API
use penrose::{
    pure::geometry::Rect as PRect,
    x::{Atom, WinType, XConn},
    x11rb::RustConn,
};
use x11_draw::{Draw, Rect};

const DX: u32 = 100;
const DY: u32 = 100;
const W: u32 = 600;
const H: u32 = 60;
const FONT: &str = "ProFont For Powerline:size=12";

fn main() -> anyhow::Result<()> {
    let conn = RustConn::new()?;
    let screen_rects = conn.screen_details()?;
    let PRect { x, y, .. } = screen_rects.last().unwrap();

    let w = conn.create_window(
        WinType::InputOutput(Atom::NetWindowTypeDock),
        PRect::new(x + DX, y + DY, W, H),
        false,
    )?;

    let mut drw = Draw::new(*conn.root(), W, H, FONT)?;
    drw.add_colorscheme("border", "#a6cc70", "#fad07b")?;
    drw.add_colorscheme("primary", "#f2e5bc", "#282828")?;
    drw.add_colorscheme("secondary", "#458588", "#b16286")?;

    let r = Rect {
        x: 0,
        y: 0,
        w: W,
        h: H,
    };
    let r_txt = Rect {
        x: 10,
        y: 10,
        w: W - 20,
        h: H - 20,
    };

    let txt = "    text is great! ◈ ζ ᛄ ℚ";

    println!("Font matches for text input:");
    drw.show_font_match_for_chars(txt);

    for n in 0..4 {
        let scheme = if n % 2 == 0 { "primary" } else { "secondary" };
        let invert = n >= 2;

        drw.set_colorscheme("border")?;
        drw.fill_rect(r, invert)?;

        drw.set_colorscheme(scheme)?;
        drw.draw_text(txt, 4, r_txt, invert)?;

        drw.flush_to(*w, r);
        conn.map(w)?;
        std::thread::sleep(std::time::Duration::from_secs(1));
    }

    Ok(())
}
