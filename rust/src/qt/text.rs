//! Retained presentation text: only application-owned sources are translated.
use cxx_qt_lib::QString;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Text(Part);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Part {
    #[default]
    Empty,
    Literal(String),
    Message {
        source: &'static str,
        arguments: Vec<Text>,
    },
}

impl Text {
    pub fn source(source: &'static str) -> Self {
        Self::message(source, [])
    }

    pub fn message(source: &'static str, arguments: impl IntoIterator<Item = Self>) -> Self {
        Self(Part::Message {
            source,
            arguments: arguments.into_iter().collect(),
        })
    }

    pub fn literal(value: impl std::fmt::Display) -> Self {
        Self(Part::Literal(value.to_string()))
    }

    pub fn is_empty(&self) -> bool {
        match &self.0 {
            Part::Empty => true,
            Part::Literal(value) => value.is_empty(),
            Part::Message { source, .. } => source.is_empty(),
        }
    }

    pub fn render(&self) -> QString {
        match &self.0 {
            Part::Empty => QString::default(),
            Part::Literal(value) => QString::from(value),
            Part::Message { source, arguments } => {
                let translated = super::ffi::translate_backend(&QString::from(*source)).to_string();
                let arguments = arguments
                    .iter()
                    .map(|argument| argument.render().to_string())
                    .collect::<Vec<_>>();
                QString::from(substitute(&translated, &arguments))
            }
        }
    }
}

// Substitute numbered Qt placeholders in one pass. Chained QString::arg calls
// could interpret a later placeholder inside an already inserted path or error.
fn substitute(mut template: &str, arguments: &[String]) -> String {
    let mut result = String::new();
    while let Some(offset) = template.find('%') {
        result.push_str(&template[..offset]);
        template = &template[offset + 1..];
        let digits = template.bytes().take(2).take_while(u8::is_ascii_digit);
        let (length, number) = digits.fold((0, 0), |(length, value), digit| {
            (length + 1, value * 10 + usize::from(digit - b'0'))
        });
        if let Some(argument) = number.checked_sub(1).and_then(|index| arguments.get(index)) {
            result.push_str(argument);
            template = &template[length..];
        } else {
            result.push('%');
        }
    }
    result.push_str(template);
    result
}

#[cfg(test)]
mod tests {
    use super::substitute;

    #[test]
    fn arguments_are_literal_even_when_reordered_or_repeated() {
        assert_eq!(
            substitute(
                "%2: %1 / %1 (%3, 100%)",
                &["path %2 日本語".into(), "cause %1".into()]
            ),
            "cause %1: path %2 日本語 / path %2 日本語 (%3, 100%)"
        );
    }
}
