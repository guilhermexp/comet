# Design: Context usage continuity

`Session.context_usage` remains the single per-chat authority. Fresh process
startup no longer writes `None`; a later `AgentEvent::Usage` continues to
replace and publish the snapshot through the existing engine/UI path. Chats
that never received usage remain `None` and keep the neutral indicator.
