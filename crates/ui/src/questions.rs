//! Native question history; interactive controls remain in the composer.
use zeron_doc::MessagePart;
use zeron_proto::{ToolCall, UserInputAnswer, UserInputQuestion};

#[derive(Clone, Debug, PartialEq)]
pub struct QuestionHistory {
    pub entries: Vec<(String, String)>,
    pub waiting: Option<String>,
    pub loading: bool,
    pub error: Option<String>,
}

impl QuestionHistory {
    pub fn input(
        questions: &[UserInputQuestion],
        answers: Option<&[UserInputAnswer]>,
        resolved: bool,
    ) -> Self {
        Self {
            entries: if resolved {
                questions
                    .iter()
                    .map(|question| {
                        let answer = match answers {
                            None => "Answer not recorded".to_owned(),
                            Some(answers) => answers
                                .iter()
                                .find(|a| a.question_id == question.id)
                                .filter(|a| !a.labels.is_empty())
                                .map(|a| a.labels.join(", "))
                                .unwrap_or_else(|| "Skipped".to_owned()),
                        };
                        (question.question.clone(), answer)
                    })
                    .collect()
            } else {
                Vec::new()
            },
            waiting: (!resolved).then(|| {
                questions
                    .first()
                    .map(|q| q.question.clone())
                    .unwrap_or_else(|| "Question".into())
            }),
            loading: false,
            error: None,
        }
    }
}

pub fn is_question(call: &ToolCall) -> bool {
    matches!(call, ToolCall::Unknown { name, .. } if matches!(name.as_str(), "ask" | "AskUserQuestion" | "request_user_input"))
}

fn tool_questions(call: &ToolCall) -> Vec<String> {
    let ToolCall::Unknown {
        input: Some(input), ..
    } = call
    else {
        return Vec::new();
    };
    if let Some(questions) = input.get("questions").and_then(|v| v.as_array()) {
        questions
            .iter()
            .filter_map(|q| {
                q.get("question")
                    .or_else(|| q.get("title"))
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
            .collect()
    } else {
        input
            .get("question")
            .or_else(|| input.get("title"))
            .and_then(|v| v.as_str())
            .map(|s| vec![s.to_owned()])
            .unwrap_or_default()
    }
}

/// Exact question text plus the enclosing tool interval prevents one request
/// from swallowing another concurrent call. Uncertain associations stay visible.
pub fn associated_inputs(parts: &[MessagePart], tool_ix: usize) -> Vec<usize> {
    let MessagePart::Tool { call, .. } = &parts[tool_ix] else {
        return Vec::new();
    };
    if !is_question(call) {
        return Vec::new();
    }
    let expected = tool_questions(call);
    parts
        .iter()
        .enumerate()
        .skip(tool_ix + 1)
        .take_while(|(_, part)| !matches!(part, MessagePart::Tool { .. }))
        .filter_map(|(ix, part)| match part {
            MessagePart::Input { questions, .. }
                if !questions.is_empty()
                    && questions.iter().all(|q| expected.contains(&q.question)) =>
            {
                Some(ix)
            }
            _ => None,
        })
        .collect()
}

pub fn tool_history(
    parts: &[MessagePart],
    tool_ix: usize,
    linked: &[usize],
    streaming: bool,
) -> QuestionHistory {
    let MessagePart::Tool {
        call,
        resolved,
        is_error,
        output,
        ..
    } = &parts[tool_ix]
    else {
        unreachable!()
    };
    let mut history = QuestionHistory {
        entries: Vec::new(),
        waiting: None,
        loading: false,
        error: None,
    };
    for ix in linked {
        if let MessagePart::Input {
            questions,
            answers,
            resolved,
            ..
        } = &parts[*ix]
        {
            let input = QuestionHistory::input(questions, answers.as_deref(), *resolved);
            for (question, answer) in input.entries {
                if let Some(entry) = history
                    .entries
                    .iter_mut()
                    .find(|(text, _)| *text == question)
                {
                    entry.1 = answer;
                } else {
                    history.entries.push((question, answer));
                }
            }
            if history.waiting.is_none() {
                history.waiting = input.waiting;
            }
        }
    }
    if *is_error || (!resolved && !streaming) {
        history.error = Some(
            output
                .clone()
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "Question interrupted".into()),
        );
    }
    if !linked.is_empty() {
        return history;
    }
    if !resolved && streaming {
        history.loading = true;
        return history;
    }
    let questions = tool_questions(call);
    let structured = output
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok());
    let answers = structured
        .as_ref()
        .and_then(|v| v.get("answers"))
        .and_then(|v| v.as_object());
    for question in &questions {
        let answer = answers
            .and_then(|a| a.get(question))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            // OMP's single-choice textual result is unambiguous only for one question.
            .or_else(|| {
                (questions.len() == 1)
                    .then(|| {
                        output
                            .as_deref()?
                            .strip_prefix("User selected: ")
                            .map(str::to_owned)
                    })
                    .flatten()
            })
            .unwrap_or_else(|| "Answer not recorded".into());
        history.entries.push((question.clone(), answer));
    }
    history
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input(resolved: bool, answers: Option<Vec<UserInputAnswer>>) -> MessagePart {
        MessagePart::Input {
            id: "input".into(),
            request_id: "r".into(),
            questions: vec![q("q1", "Pick one")],
            answers,
            resolved,
        }
    }
    fn q(id: &str, text: &str) -> UserInputQuestion {
        UserInputQuestion {
            id: id.into(),
            header: "Choice".into(),
            question: text.into(),
            options: vec![],
            multi_select: false,
        }
    }
    fn tool() -> MessagePart {
        serde_json::from_value(serde_json::json!({"kind":"tool", "id":"ask", "call":{"kind":"unknown", "name":"ask", "input":{"question":"Pick one"}}, "resolved":true, "isError":false})).unwrap()
    }
    #[test]
    fn answers_follow_question_order_and_ids() {
        let questions = vec![q("1", "First?"), q("2", "Second?")];
        let answers = vec![
            UserInputAnswer {
                question_id: "2".into(),
                labels: vec!["B".into(), "C".into()],
            },
            UserInputAnswer {
                question_id: "1".into(),
                labels: vec!["A".into()],
            },
        ];
        let h = QuestionHistory::input(&questions, Some(&answers), true);
        assert_eq!(
            h.entries,
            vec![
                ("First?".into(), "A".into()),
                ("Second?".into(), "B, C".into())
            ]
        );
        assert_eq!(
            QuestionHistory::input(&questions, None, true).entries[0].1,
            "Answer not recorded"
        );
        assert_eq!(
            QuestionHistory::input(&questions, Some(&[]), true).entries[0].1,
            "Skipped"
        );
        assert_eq!(
            QuestionHistory::input(&questions, None, false)
                .waiting
                .as_deref(),
            Some("First?")
        );
    }
    #[test]
    fn only_matching_requests_replace_question_tools() {
        let mut parts = vec![tool(), input(false, None)];
        assert_eq!(associated_inputs(&parts, 0), vec![1]);
        let h = tool_history(&parts, 0, &[1], true);
        assert_eq!(h.waiting.as_deref(), Some("Pick one"));
        assert!(!h.loading);
        if let MessagePart::Input { questions, .. } = &mut parts[1] {
            questions[0].question = "Unrelated".into();
        }
        assert!(associated_inputs(&parts, 0).is_empty());
    }
}

pub fn render(
    history: &QuestionHistory,
    theme: &crate::theme::Theme,
    view: gpui::EntityId,
    cx: &mut gpui::App,
) -> gpui::AnyElement {
    use gpui::{div, prelude::*, px};
    let mut body = div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .py(px(4.0))
        .text_size(px(14.0))
        .line_height(px(22.0))
        .font_weight(gpui::FontWeight::NORMAL);
    if history.loading {
        let phase = crate::motion::pulse_delta(&crate::motion::ACTIVITY_SHIMMER, view, cx);
        let opacity = 0.55 + 0.45 * (phase * std::f32::consts::PI).sin();
        body = body.child(
            div()
                .text_color(theme.text_muted.opacity(opacity))
                .child("Asking question…"),
        );
    }
    if let Some(question) = &history.waiting {
        body = body.child(
            div()
                .flex()
                .flex_wrap()
                .gap(px(6.0))
                .child(div().text_color(theme.text_muted).child(question.clone()))
                .child(
                    div()
                        .text_color(theme.text_faint)
                        .child("• Waiting for response…"),
                ),
        );
    }
    if !history.entries.is_empty() {
        body = body.child(
            div()
                .w_full()
                .rounded(px(8.0))
                .border_1()
                .border_color(theme.border)
                .bg(crate::theme::ink(0.025))
                .overflow_hidden()
                .child(
                    div()
                        .h(px(28.0))
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .border_b_1()
                        .border_color(theme.border)
                        .child(
                            crate::icons::icon(crate::icons::CHAT_ROUND_LINE)
                                .size(px(14.0))
                                .text_color(theme.text_muted),
                        )
                        .child(div().text_color(theme.text_muted).child(
                            if history.entries.len() == 1 {
                                "Answer"
                            } else {
                                "Answers"
                            },
                        )),
                )
                .child(div().p(px(10.0)).flex().flex_col().gap(px(8.0)).children(
                    history.entries.iter().map(|(question, answer)| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_color(theme.text)
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .child(question.clone()),
                            )
                            .child(div().text_color(theme.text_faint).child(answer.clone()))
                    }),
                )),
        );
    }
    if let Some(error) = &history.error {
        body = body.child(div().text_color(theme.danger).child(error.clone()));
    }
    if !history.loading
        && history.waiting.is_none()
        && history.entries.is_empty()
        && history.error.is_none()
    {
        body = body.child(
            div()
                .text_color(theme.text_faint)
                .child("Answer not recorded"),
        );
    }
    body.into_any_element()
}
