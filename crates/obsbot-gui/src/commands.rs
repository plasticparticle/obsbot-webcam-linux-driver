//! Commands the app accepts from outside the window: the tray menu and the
//! `obsbotctl app …` command line (and so any desktop keyboard shortcut).

use crate::effects::Hold;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Switch {
    On,
    Off,
    Toggle,
}

impl Switch {
    pub fn apply(self, current: bool) -> bool {
        match self {
            Switch::On => true,
            Switch::Off => false,
            Switch::Toggle => !current,
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum Command {
    Show,
    /// Close the window but keep running in the background.
    Hide,
    Quit,
    /// Apply the profile with this name.
    Profile(String),
    /// Step forwards (+1) or backwards (-1) through the profiles.
    ProfileStep(i32),
    Effects(Switch),
    /// Show a stand-in picture; an optional text replaces the card's message.
    Hold(Hold, Switch, Option<String>),
    /// Camera sleep ("privacy mode").
    Privacy(Switch),
    Position(String),
}

pub const HELP: &str =
    "show | hide | quit | status | profile <name> | profile next | profile previous | \
effects on|off|toggle | brb [on|off|toggle] [text] | freeze [on|off|toggle] | \
privacy on|off|toggle | position <name>";

fn switch(word: Option<&str>) -> Result<Switch, String> {
    match word {
        None | Some("toggle") => Ok(Switch::Toggle),
        Some("on") => Ok(Switch::On),
        Some("off") => Ok(Switch::Off),
        Some(other) => Err(format!("expected on, off or toggle, not '{other}'")),
    }
}

/// Parse one command line.
pub fn parse(line: &str) -> Result<Command, String> {
    let line = line.trim();
    let (verb, rest) = match line.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest.trim()),
        None => (line, ""),
    };
    let named = |what: &str| -> Result<String, String> {
        if rest.is_empty() {
            Err(format!("which {what}?"))
        } else {
            Ok(rest.to_owned())
        }
    };
    match verb {
        "show" => Ok(Command::Show),
        "hide" => Ok(Command::Hide),
        "quit" => Ok(Command::Quit),
        "profile" => match rest {
            "next" => Ok(Command::ProfileStep(1)),
            "previous" | "prev" => Ok(Command::ProfileStep(-1)),
            _ => named("profile").map(Command::Profile),
        },
        "effects" => switch(rest.split_whitespace().next()).map(Command::Effects),
        "privacy" => switch(rest.split_whitespace().next()).map(Command::Privacy),
        "freeze" => {
            switch(rest.split_whitespace().next()).map(|s| Command::Hold(Hold::Freeze, s, None))
        }
        "brb" => {
            // `brb`, `brb off`, `brb Back at 3`, `brb on Back at 3`
            let (first, tail) = match rest.split_once(char::is_whitespace) {
                Some((first, tail)) => (first, tail.trim()),
                None => (rest, ""),
            };
            match first {
                "on" | "off" | "toggle" => Ok(Command::Hold(
                    Hold::Card,
                    switch(Some(first))?,
                    (!tail.is_empty()).then(|| tail.to_owned()),
                )),
                "" => Ok(Command::Hold(Hold::Card, Switch::Toggle, None)),
                _ => Ok(Command::Hold(Hold::Card, Switch::On, Some(rest.to_owned()))),
            }
        }
        "position" => named("position").map(Command::Position),
        "" => Err(format!("no command. Try: {HELP}")),
        other => Err(format!("unknown command '{other}'. Try: {HELP}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_parse_with_and_without_arguments() {
        assert_eq!(parse("show"), Ok(Command::Show));
        assert_eq!(parse("hide"), Ok(Command::Hide));
        assert_eq!(
            parse(" profile  Late night "),
            Ok(Command::Profile("Late night".into()))
        );
        assert_eq!(parse("profile next"), Ok(Command::ProfileStep(1)));
        assert_eq!(parse("profile prev"), Ok(Command::ProfileStep(-1)));
        assert_eq!(parse("effects"), Ok(Command::Effects(Switch::Toggle)));
        assert_eq!(parse("effects off"), Ok(Command::Effects(Switch::Off)));
        assert_eq!(parse("privacy on"), Ok(Command::Privacy(Switch::On)));
        assert_eq!(
            parse("freeze"),
            Ok(Command::Hold(Hold::Freeze, Switch::Toggle, None))
        );
        assert_eq!(
            parse("position Whiteboard"),
            Ok(Command::Position("Whiteboard".into()))
        );
    }

    #[test]
    fn brb_accepts_a_switch_a_message_or_both() {
        assert_eq!(
            parse("brb"),
            Ok(Command::Hold(Hold::Card, Switch::Toggle, None))
        );
        assert_eq!(
            parse("brb off"),
            Ok(Command::Hold(Hold::Card, Switch::Off, None))
        );
        assert_eq!(
            parse("brb Back at 3"),
            Ok(Command::Hold(
                Hold::Card,
                Switch::On,
                Some("Back at 3".into())
            ))
        );
        assert_eq!(
            parse("brb on Lunch"),
            Ok(Command::Hold(Hold::Card, Switch::On, Some("Lunch".into())))
        );
    }

    #[test]
    fn mistakes_are_explained() {
        assert!(parse("profile").unwrap_err().contains("which profile"));
        assert!(parse("effects maybe")
            .unwrap_err()
            .contains("on, off or toggle"));
        assert!(parse("dance").unwrap_err().contains("unknown command"));
        assert!(parse("").is_err());
        assert!(!Switch::Toggle.apply(true));
        assert!(Switch::On.apply(false));
    }
}
