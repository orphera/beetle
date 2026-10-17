//! Korean particles that follow a name: 을/를 and 은/는 depend on whether the
//! name's last sound ends in a consonant.

/// Whether `name` ends in a consonant sound. Hangul is exact; digits and Latin
/// letters follow how they are usually read ("3" is 삼, "L" is 엘).
fn has_final_consonant(name: &str) -> bool {
    let Some(last) = name
        .chars()
        .rev()
        .find(|c| c.is_alphanumeric() || ('\u{ac00}'..='\u{d7a3}').contains(c))
    else {
        return false;
    };
    match last {
        '\u{ac00}'..='\u{d7a3}' => (last as u32 - 0xac00) % 28 != 0,
        '0' | '1' | '3' | '6' | '7' | '8' => true,
        'l' | 'm' | 'n' | 'L' | 'M' | 'N' => true,
        _ => false,
    }
}

/// `'name'` followed by the object particle (을/를).
pub fn quoted_eul(name: &str) -> String {
    let p = if has_final_consonant(name) {
        "을"
    } else {
        "를"
    };
    format!("'{name}'{p}")
}

/// `'name'` followed by the topic particle (은/는).
pub fn quoted_eun(name: &str) -> String {
    let p = if has_final_consonant(name) {
        "은"
    } else {
        "는"
    };
    format!("'{name}'{p}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_particle_by_the_last_sound() {
        assert_eq!(quoted_eul("곡"), "'곡'을");
        assert_eq!(quoted_eul("나비"), "'나비'를");
        assert_eq!(quoted_eul("FREEDOM DiVE"), "'FREEDOM DiVE'를");
        assert_eq!(quoted_eul("Air"), "'Air'를");
        assert_eq!(quoted_eul("Sample Song 3"), "'Sample Song 3'을");
        assert_eq!(quoted_eun("Blue Zenith"), "'Blue Zenith'는");
        assert_eq!(quoted_eun("Moon Man"), "'Moon Man'은");
    }
}
