//! EVE-Simulator für Tests unter Linux (X11/Xvfb): öffnet Fenster, die sich wie
//! EVE-Clients unter Proton ausgeben (Titel „EVE - Name“, WM_CLASS exefile.exe).
//!
//! Aufruf: `cargo run --example fake_eve -- "Pilot One" "Pilot Two" LOGIN`
//! („LOGIN“ erzeugt einen Client am Login-Bildschirm mit dem Titel „EVE“.)
//! Jedes Fenster zeigt eine eigene Farbe mit Streifen und seinen Namen,
//! damit die Live-Vorschau eindeutig zuzuordnen ist.

#[cfg(target_os = "linux")]
fn main() {
    linux::run();
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("Nur unter Linux/X11 verfügbar.");
}

#[cfg(target_os = "linux")]
mod linux {
    use x11rb::connection::Connection;
    use x11rb::protocol::Event;
    use x11rb::protocol::xproto::*;
    use x11rb::wrapper::ConnectionExt as _;

    /// Kräftige, gut unterscheidbare Farben (0xRRGGBB).
    const COLORS: [u32; 6] = [0xC0392B, 0x2980B9, 0x27AE60, 0x8E44AD, 0xD35400, 0x16A085];

    /// Ein simuliertes Client-Fenster.
    struct Fake {
        win: Window,
        gc: Gcontext,
        color: u32,
        label: String,
    }

    /// Legt die Fenster an und zeichnet sie bei jedem Expose neu, bis das Programm beendet wird.
    pub fn run() {
        let names: Vec<String> = std::env::args().skip(1).collect();
        let names = if names.is_empty() {
            vec!["Pilot One".to_string(), "Pilot Two".to_string()]
        } else {
            names
        };
        let (conn, screen_num) = x11rb::connect(None).expect("kein X-Server ($DISPLAY)");
        let screen = conn.setup().roots[screen_num].clone();
        let utf8 = conn.intern_atom(false, b"UTF8_STRING").unwrap().reply().unwrap().atom;
        let net_name = conn.intern_atom(false, b"_NET_WM_NAME").unwrap().reply().unwrap().atom;
        let net_pid = conn.intern_atom(false, b"_NET_WM_PID").unwrap().reply().unwrap().atom;
        let font = conn.generate_id().unwrap();
        conn.open_font(font, b"fixed").unwrap();

        let mut fakes = Vec::new();
        for (i, name) in names.iter().enumerate() {
            let title = if name == "LOGIN" {
                "EVE".to_string()
            } else {
                format!("EVE - {name}")
            };
            let win = conn.generate_id().unwrap();
            let (x, y) = (100 + 60 * i as i16, 100 + 60 * i as i16);
            let aux = CreateWindowAux::new()
                .background_pixel(COLORS[i % COLORS.len()])
                .event_mask(EventMask::EXPOSURE | EventMask::STRUCTURE_NOTIFY);
            conn.create_window(
                0,
                win,
                screen.root,
                x,
                y,
                800,
                450,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &aux,
            )
            .unwrap();
            conn.change_property8(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                title.as_bytes(),
            )
            .unwrap();
            conn.change_property8(PropMode::REPLACE, win, net_name, utf8, title.as_bytes())
                .unwrap();
            conn.change_property8(
                PropMode::REPLACE,
                win,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"exefile.exe\0Wine\0",
            )
            .unwrap();
            conn.change_property32(
                PropMode::REPLACE,
                win,
                net_pid,
                AtomEnum::CARDINAL,
                &[std::process::id()],
            )
            .unwrap();
            let gc = conn.generate_id().unwrap();
            conn.create_gc(
                gc,
                win,
                &CreateGCAux::new().font(font).foreground(0xFFFFFF).background(0),
            )
            .unwrap();
            conn.map_window(win).unwrap();
            fakes.push(Fake {
                win,
                gc,
                color: COLORS[i % COLORS.len()],
                label: title,
            });
        }
        conn.flush().unwrap();
        println!(
            "{} Fake-Clients gestartet: {:?}",
            fakes.len(),
            fakes.iter().map(|f| f.win).collect::<Vec<_>>()
        );

        while let Ok(ev) = conn.wait_for_event() {
            if let Event::Expose(e) = ev
                && let Some(f) = fakes.iter().find(|f| f.win == e.window)
            {
                draw(&conn, f);
            }
        }
    }

    /// Zeichnet Hintergrund, helle Streifen und den Titel.
    fn draw(conn: &impl Connection, f: &Fake) {
        let geo = conn.get_geometry(f.win).unwrap().reply().unwrap();
        conn.change_gc(f.gc, &ChangeGCAux::new().foreground(f.color)).unwrap();
        conn.poly_fill_rectangle(
            f.win,
            f.gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: geo.width,
                height: geo.height,
            }],
        )
        .unwrap();
        conn.change_gc(f.gc, &ChangeGCAux::new().foreground(0xFFFFFF)).unwrap();
        let stripes: Vec<Rectangle> = (0..geo.width / 80)
            .map(|i| Rectangle {
                x: (i * 80) as i16,
                y: 0,
                width: 20,
                height: geo.height,
            })
            .collect();
        conn.poly_fill_rectangle(f.win, f.gc, &stripes).unwrap();
        conn.change_gc(f.gc, &ChangeGCAux::new().foreground(0xFFFFFF).background(0))
            .unwrap();
        conn.image_text8(f.win, f.gc, 40, 60, f.label.as_bytes()).unwrap();
        conn.flush().unwrap();
    }
}
