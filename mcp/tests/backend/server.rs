use super::*;
use crate::{
    testing::{StandInActions, StandInRead},
    toolbox::{Answered, Image, ImageType},
};
use serde_json::Value;

fn called(answered: Answered) -> Called {
    Called {
        answer: Ok(answered),
        dsp: None,
    }
}
fn texts(result: &CallToolResult) -> Vec<Value> {
    serde_json::to_value(&result.content)
        .unwrap()
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn an_answer_sends_its_data_then_its_words_then_its_pictures() {
    let reply = called(Answered {
        data: json!({"count": 2}),
        text: Some("Two of them.".into()),
        images: vec![Image {
            mime: ImageType::Png,
            bytes: vec![1, 2, 3],
        }],
    });
    let (result, outcome) = answered("count", &reply, true);
    assert_eq!(outcome, "ok");
    assert_eq!(result.structured_content, Some(json!({"count": 2})));
    let content = texts(&result);
    assert_eq!(content[0]["text"], "{\"count\":2}");
    assert_eq!(content[1]["text"], "Two of them.");
    assert_eq!(content[2]["type"], "image");
    assert_eq!(content[2]["data"], "AQID");
    assert_eq!(content[2]["mimeType"], "image/png");
    // A client from before structured results reads the same content, without them.
    let (older, _) = answered("count", &reply, false);
    assert_eq!(older.structured_content, None);
    assert_eq!(texts(&older), content);
}

#[test]
fn an_answer_too_large_to_send_is_refused_whole() {
    let words = called(Answered {
        data: json!({}),
        text: Some("x".repeat(LONGEST_ANSWER)),
        images: vec![],
    });
    assert_eq!(answered("long", &words, true).1, "answer_too_large");
    let pictures = called(Answered {
        data: json!({}),
        text: None,
        images: vec![Image {
            mime: ImageType::Jpeg,
            bytes: vec![0; LARGEST_IMAGES + 1],
        }],
    });
    assert_eq!(answered("large", &pictures, true).1, "answer_too_large");
}

#[test]
fn a_connection_that_only_reads_sees_a_tool_that_changes_as_reading() {
    let reading = tool(
        Offered {
            tool: &StandInActions,
            level: ToolLevel::Read,
        },
        true,
    );
    let hints = reading.annotations.clone().unwrap();
    assert_eq!(hints.read_only_hint, Some(true));
    assert!(
        reading
            .description
            .as_deref()
            .unwrap()
            .ends_with("This connection may only read with it: what changes something is refused."),
        "{reading:?}"
    );
    let changing = tool(
        Offered {
            tool: &StandInActions,
            level: ToolLevel::Change,
        },
        true,
    );
    assert_eq!(changing.annotations.unwrap().read_only_hint, Some(false));
    assert_eq!(
        changing.description.as_deref(),
        Some("Look at the DSP, or mark it.")
    );
    let reader = tool(
        Offered {
            tool: &StandInRead,
            level: ToolLevel::Read,
        },
        true,
    );
    assert_eq!(reader.annotations.unwrap().read_only_hint, Some(true));
}
