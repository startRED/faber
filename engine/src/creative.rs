//! Bounded, blind creative comparisons. No CLI is started by this module.
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub request: String,
    pub rubric: String,
    pub first: String,
    pub second: String,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Score {
    pub a: u8,
    pub b: u8,
    pub reason: String,
}

/// Numbers refer to the original drafts; the reason retains the judge's shown labels.
#[derive(Debug, Serialize, PartialEq)]
pub struct Revealed {
    pub first: u8,
    pub second: u8,
    pub a_is: &'static str,
    pub reason: String,
}

impl Request {
    pub fn parse(input: &serde_json::Value) -> Option<Self> {
        let fields = input.as_object()?;
        if fields.len() != 4 {
            return None;
        }
        // Bound the caller's data before cloning it into the owned request.
        for (key, limit) in [("request", 8192), ("rubric", 4096), ("first", 65_536), ("second", 65_536)] {
            let text = fields.get(key)?.as_str()?;
            if text.trim().is_empty() || text.len() > limit {
                return None;
            }
        }
        serde_json::from_value(input.clone()).ok()
    }

    pub fn prompt(&self, swapped: bool) -> String {
        let (a, b) = if swapped { (&self.second, &self.first) } else { (&self.first, &self.second) };
        serde_json::json!({ "request": self.request, "rubric": self.rubric, "A": a, "B": b }).to_string()
    }
}

impl Score {
    pub fn parse(text: &str) -> Option<Self> {
        if text.len() > 4096 {
            return None;
        }
        let value: Self = serde_json::from_str(text).ok()?;
        ((1..=10).contains(&value.a) && (1..=10).contains(&value.b) && !value.reason.trim().is_empty() && value.reason.len() <= 2000)
            .then_some(value)
    }

    pub fn reveal(self, swapped: bool) -> Revealed {
        let (first, second, a_is) = if swapped { (self.b, self.a, "second") } else { (self.a, self.b, "first") };
        Revealed { first, second, a_is, reason: self.reason }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn input() -> serde_json::Value {
        json!({"request":"Write a headline", "rubric":"Clarity and accuracy", "first":"First draft", "second":"Second draft"})
    }

    #[test]
    fn creative_accepts_only_a_complete_bounded_comparison() {
        assert!(Request::parse(&input()).is_some());
        for key in ["request", "rubric", "first", "second"] {
            let mut value = input();
            value[key] = json!(" \n ");
            assert!(Request::parse(&value).is_none(), "empty {key}");
            value[key] = json!("x".repeat(65_537));
            assert!(Request::parse(&value).is_none(), "oversized {key}");
        }
        let mut value = input();
        value["author"] = json!("Hidden vendor identity");
        assert!(Request::parse(&value).is_none());
    }

    #[test]
    fn creative_blind_labels_reverse_without_leaking_mapping() {
        let request = Request::parse(&input()).unwrap();
        let normal: serde_json::Value = serde_json::from_str(&request.prompt(false)).unwrap();
        let reversed: serde_json::Value = serde_json::from_str(&request.prompt(true)).unwrap();
        assert_eq!(normal, json!({"request":"Write a headline", "rubric":"Clarity and accuracy", "A":"First draft", "B":"Second draft"}));
        assert_eq!(normal["A"], reversed["B"]);
        assert_eq!(normal["B"], reversed["A"]);
    }

    #[test]
    fn creative_requires_strict_scores_and_maps_them_back() {
        let score = Score::parse(r#"{"a":3,"b":9,"reason":"B is clearer."}"#).unwrap();
        assert_eq!(score.reveal(true), Revealed { first: 9, second: 3, a_is: "second", reason: "B is clearer.".into() });
        assert!(Score::parse(r#"{"a":10,"b":1,"reason":"Clear."}"#).is_some());
        for text in [
            r#"{"a":0,"b":9,"reason":"Invalid."}"#,
            r#"{"a":11,"b":9,"reason":"Invalid."}"#,
            r#"{"a":3.5,"b":9,"reason":"Invalid."}"#,
            r#"{"a":3,"b":9,"reason":" "}"#,
            r#"{"a":3,"b":9,"reason":"Ok.","winner":"B"}"#,
            r#"Some text {"a":3,"b":9,"reason":"Ok."}"#,
            r#"{"a":3,"b":9,"reason":"Ok."} trailing"#,
        ] {
            assert!(Score::parse(text).is_none(), "{text}");
        }
        assert!(Score::parse(&json!({"a":3,"b":9,"reason":"x".repeat(2001)}).to_string()).is_none());
    }

    #[test]
    fn creative_records_the_shown_order_with_the_original_reason() {
        let score = Score::parse(r#"{"a":3,"b":9,"reason":"B is clearer."}"#).unwrap();
        assert_eq!(
            serde_json::to_value(score.reveal(true)).unwrap(),
            json!({"first":9,"second":3,"a_is":"second","reason":"B is clearer."})
        );
    }
}
