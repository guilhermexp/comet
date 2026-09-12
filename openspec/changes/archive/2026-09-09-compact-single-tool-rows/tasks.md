## 1. Transcript event presentation
- [x] 1.1 Reproduce redundant disclosures with failing unit coverage.
- [x] 1.2 Render lone tools directly; keep multi-tool groups compact until explicitly expanded.
- [x] 1.3 Project reasoning content previews once; preserve full Markdown and explicit disclosure choices.
- [x] 1.4 Centralize event geometry and typography, remove connector spines, align detail indentation and collapse task snapshots by default.
- [x] 1.5 Update UI owner contracts and design guidance to match the new presentation.
- [x] 1.6 Run UI tests (1185 passed), workspace suite and final native build.
- [x] 1.7 Inspect the original reported Chat in the headed review build; verify compact event rows and expansion of reasoning and task details.

Validation: the final workspace run passed. The earlier RPC timeout passed on isolated retry and did not recur in the final workspace run. Native review showed the original report Chat with compact, aligned events; reasoning and task disclosures expanded beneath their summaries. The user's active window was left alone after the inspector reported a user-driven application change. No messages were submitted to the real profile; generated runs used the isolated mock daemon.
