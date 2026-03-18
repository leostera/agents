#[macro_export]
macro_rules! typed_tool {
    (
        $name:ident,
        [
            $(
                $variant:ident $( ($arg:ty) )?
                => $description:literal
            ),* $(,)?
        ]
    ) => {
        impl TypedTool for $name {
            fn tool_name(&self) -> &'static str {
                stringify!($name)
            }

            fn variants() -> &'static [ToolVariant] {
                const _: () = {
                    fn to_snake_case(s: &str) -> &'static str {
                        let mut result = String::new();
                        for (i, c) in s.chars().enumerate() {
                            if c.is_uppercase() {
                                if i > 0 {
                                    result.push('_');
                                }
                                result.push(c.to_lowercase().next().unwrap());
                            } else {
                                result.push(c);
                            }
                        }
                        let mut owned = result;
                        Box::leak(owned.into_boxed_str())
                    }

                    fn make_variants() -> [ToolVariant; { 0 $( + 1 )* }] {
                        [
                            $(
                                ToolVariant {
                                    name: to_snake_case(stringify!($variant)),
                                    description: $description,
                                }
                            ),*
                        ]
                    }
                };

                static VARIANTS: [ToolVariant; { 0 $( + 1 )* }] = [
                    $(
                        {
                            fn to_snake_case(s: &str) -> &'static str {
                                let mut result = String::new();
                                for (i, c) in s.chars().enumerate() {
                                    if c.is_uppercase() {
                                        if i > 0 {
                                            result.push('_');
                                        }
                                        result.push(c.to_lowercase().next().unwrap());
                                    } else {
                                        result.push(c);
                                    }
                                }
                                Box::leak(result.into_boxed_str())
                            }

                            ToolVariant {
                                name: to_snake_case(stringify!($variant)),
                                description: $description,
                            }
                        }
                    ),*
                ];

                &VARIANTS
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    enum TestTools {
        #[allow(dead_code)]
        WhoAmI,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
    enum TestToolsWithSchema {
        #[allow(dead_code)]
        WhoAmI,
    }

    #[test]
    fn test_typed_tool_macro() {
        typed_tool!(TestTools, [
            WhoAmI => "Get user info",
        ]);

        let tool = TestTools::WhoAmI;
        assert_eq!(tool.tool_name(), "TestTools");
    }
}
