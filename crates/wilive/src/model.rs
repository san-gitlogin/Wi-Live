//! What the tray says, decided from the server's state. Pure, so every wording is
//! tested without a tray.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Idle,
    /// Someone is watching: the red dot (R-25).
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub icon: Icon,
    /// Plain status lines at the top of the menu.
    pub lines: Vec<String>,
    pub paused: bool,
    /// Requests waiting for the owner: the window opens when this grows.
    pub asking: usize,
    pub tooltip: String,
}

pub struct Input<'a> {
    pub watching: &'a [String],
    pub asking: &'a [String],
    pub paused: bool,
    pub capture_allowed: bool,
    pub url: Option<&'a str>,
}

fn names(n: &[String]) -> String {
    match n.len() {
        0 => String::new(),
        1..=3 => n.join(", "),
        k => format!("{}, {} and {} more", n[0], n[1], k - 2),
    }
}

pub fn model(i: &Input) -> Model {
    let mut lines = Vec::new();
    if i.paused {
        lines.push("Sharing is paused".to_string());
    } else if i.watching.is_empty() {
        lines.push("Nobody is watching".to_string());
    } else {
        lines.push(format!("Watching: {}", names(i.watching)));
    }
    for a in i.asking {
        lines.push(format!("{a} wants to watch"));
    }
    if !i.capture_allowed {
        lines.push("Screen recording is not allowed yet".to_string());
    }
    match i.url {
        Some(u) if !i.paused => lines.push(format!("Open {u} on a device")),
        None => lines.push("Not on a home network".to_string()),
        _ => {}
    }
    let tooltip = match (i.paused, i.watching.len()) {
        (true, _) => "Wi-Live: paused".to_string(),
        (false, 0) => "Wi-Live".to_string(),
        (false, 1) => format!("Wi-Live: {} is watching", i.watching[0]),
        (false, n) => format!("Wi-Live: {n} watching"),
    };
    Model {
        icon: if i.watching.is_empty() || i.paused {
            Icon::Idle
        } else {
            Icon::Live
        },
        lines,
        paused: i.paused,
        asking: i.asking.len(),
        tooltip,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn nobody_watching_shows_where_to_open_it_and_no_red_dot() {
        let m = model(&Input {
            watching: &[],
            asking: &[],
            paused: false,
            capture_allowed: true,
            url: Some("http://192.168.0.5:47900/"),
        });
        assert_eq!(m.icon, Icon::Idle);
        assert_eq!(
            m.lines,
            s(&[
                "Nobody is watching",
                "Open http://192.168.0.5:47900/ on a device"
            ])
        );
    }

    #[test]
    fn anyone_watching_turns_the_dot_red_and_names_them() {
        let w = s(&["Pixel 7", "iPad"]);
        let m = model(&Input {
            watching: &w,
            asking: &[],
            paused: false,
            capture_allowed: true,
            url: Some("u"),
        });
        assert_eq!(m.icon, Icon::Live);
        assert_eq!(m.lines[0], "Watching: Pixel 7, iPad");
        assert_eq!(m.tooltip, "Wi-Live: 2 watching");
        let many = s(&["A", "B", "C", "D", "E"]);
        assert_eq!(
            model(&Input {
                watching: &many,
                asking: &[],
                paused: false,
                capture_allowed: true,
                url: None
            })
            .lines[0],
            "Watching: A, B and 3 more"
        );
    }

    #[test]
    fn requests_permission_and_pause_are_said_plainly() {
        let a = s(&["Living room TV"]);
        let m = model(&Input {
            watching: &[],
            asking: &a,
            paused: true,
            capture_allowed: false,
            url: Some("u"),
        });
        assert_eq!(
            m.lines,
            s(&[
                "Sharing is paused",
                "Living room TV wants to watch",
                "Screen recording is not allowed yet"
            ])
        );
        assert_eq!(m.asking, 1);
        assert_eq!(m.icon, Icon::Idle);
    }
}
