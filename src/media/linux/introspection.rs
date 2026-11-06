use super::mpris::Interface;

const INTERFACE_END: &str = "</interface>";
const PROPERTY_TAG: &str = "<property";
const TAG_END: char = '>';
const SELF_CLOSING: char = '/';
const WRITABLE_ACCESS: [&str; 2] = ["access=\"readwrite\"", "access=\"write\""];

pub(super) fn property_writable(xml: &str, interface: Interface, property: &str) -> bool {
    let opening = format!("<interface name=\"{}\"", interface.name());
    let Some((_, body)) = xml.split_once(opening.as_str()) else {
        return false;
    };
    let body = body
        .split_once(INTERFACE_END)
        .map_or(body, |(inside, _)| inside);
    let wanted = format!("name=\"{property}\"");
    body.split(PROPERTY_TAG)
        .skip(1)
        .filter_map(|tag| tag.split(TAG_END).next())
        .map(|tag| tag.trim_end_matches(SELF_CLOSING))
        .find(|tag| tag.split_whitespace().any(|attribute| attribute == wanted))
        .is_some_and(|tag| {
            tag.split_whitespace()
                .any(|attribute| WRITABLE_ACCESS.contains(&attribute))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writable_volume_is_read_from_introspection() {
        let xml = r#"<node>
  <interface name="org.mpris.MediaPlayer2">
    <property name="Volume" type="d" access="readwrite"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <property name="Shuffle" type="b" access="readwrite"/>
    <property name="Volume" type="d" access="read"/>
  </interface>
</node>"#;
        assert!(!property_writable(xml, Interface::Player, "Volume"));
        assert!(property_writable(xml, Interface::Player, "Shuffle"));
        assert!(!property_writable(xml, Interface::Player, "LoopStatus"));
        let writable = xml.replace(r#"access="read"/>"#, r#"access="readwrite"/>"#);
        assert!(property_writable(&writable, Interface::Player, "Volume"));
        assert!(!property_writable("", Interface::Player, "Volume"));
    }
}
