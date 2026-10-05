//! The narrowing a list takes: `release:7.2.0 priority:high key:B under:A7 area:kites` parsed once, then read as
//! the query parameters of the server's `/next` route.

/// The fields a filter line names, in the order the query and the label list them.
const FIELDS: [&str; 6] = ["release", "priority", "key", "complexity", "under", "area"];

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Filter {
    pub release: Option<String>,
    pub priority: Option<String>,
    pub key: Option<String>,
    pub complexity: Option<String>,
    pub under: Option<String>,
    /// The area's name; the server's queue does not take it, so the rows are cut by it after.
    pub area: Option<String>,
}

impl Filter {
    /// Only what a plan opened, at any depth.
    #[must_use]
    pub fn under(id: &str) -> Self {
        Self {
            under: Some(id.to_string()),
            ..Self::default()
        }
    }

    /// # Errors
    /// A word that is not `field:value`, or names a field that is not one of the five.
    pub fn parse(line: &str) -> Result<Self, String> {
        let mut f = Self::default();
        for word in line.split_whitespace() {
            let (field, value) = word
                .split_once(':')
                .filter(|(_, v)| !v.is_empty())
                .ok_or_else(|| format!("{word} is not field:value; the fields are {}", fields()))?;
            let slot = match field {
                "release" => &mut f.release,
                "priority" => &mut f.priority,
                "key" => &mut f.key,
                "complexity" => &mut f.complexity,
                "under" => &mut f.under,
                "area" => &mut f.area,
                _ => {
                    return Err(format!(
                        "unknown filter field {field}; the fields are {}",
                        fields()
                    ));
                }
            };
            *slot = Some(value.to_string());
        }
        Ok(f)
    }

    fn set(&self) -> [(&'static str, &'static str, Option<&String>); 6] {
        [
            ("release", "release", self.release.as_ref()),
            ("priority", "priority", self.priority.as_ref()),
            ("key", "key", self.key.as_ref()),
            ("complexity", "complexity", self.complexity.as_ref()),
            ("under", "under", self.under.as_ref()),
            ("area", "area", self.area.as_ref()),
        ]
    }

    /// The parameters `/next` takes.
    #[must_use]
    pub fn query(&self) -> Vec<(&'static str, String)> {
        self.set()
            .into_iter()
            .filter(|(field, _, _)| *field != "area")
            .filter_map(|(_, param, v)| v.map(|v| (param, v.clone())))
            .collect()
    }

    /// The line that parses back to this filter, empty when nothing narrows.
    #[must_use]
    pub fn label(&self) -> String {
        self.set()
            .into_iter()
            .filter_map(|(field, _, v)| v.map(|v| format!("{field}:{v}")))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn fields() -> String {
    FIELDS.join(", ")
}

#[cfg(test)]
#[path = "tests/filter.rs"]
mod tests;
