# turn-step-tool-groups Specification

## Purpose

Keep every nested tool card visible when an assistant turn's operational steps
are expanded, without opening each card's invocation, output, or diff details.

## Requirements

### Requirement: Show tool cards inside expanded turn steps
The transcript SHALL expose individual tool rows directly inside expanded `TurnSteps`, without intermediate count headers. Recorded reasoning SHALL remain available through its own compact disclosure.

#### Scenario: A completed prefix contains several tool groups
Test: unit — deterministic transcript projection; headed GPUI smoke.
- **WHEN** the outer TurnSteps disclosure is expanded
- **THEN** each individual call is visible directly without a group counter
- **AND** expanding an event reveals its recorded details

### Requirement: Keep card details independently compact

The transcript SHALL keep invocation, output, and diff detail bodies closed by
default when it opens a nested tool group.

#### Scenario: A completed command card has recorded output

Test: deterministic projection test asserting independent group and detail defaults.

- **WHEN** a completed command group becomes a `TurnSteps` child
- **THEN** the command card is visible
- **AND** its output and invocation bodies remain closed until the user opens the card

### Requirement: Preserve explicit disclosure choices
The transcript SHALL preserve explicit turn, tool-detail and reasoning fold choices across rerenders and virtualized remounts. Default compact presentation SHALL apply to both settled and streaming events without hiding subagent links.

#### Scenario: The user expands a compact group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user expands the completed turn summary that defaulted closed
- **THEN** it remains expanded across rerenders and virtualized remounts
- **AND** independent subagent links remain directly accessible

#### Scenario: The user collapses an open nested group
Test: unit — fold-state precedence and virtualized remount regression suite.
- **WHEN** the user collapses a tool detail they previously opened
- **THEN** the group remains collapsed across rerenders and virtualized remounts
- **AND** top-level tool calls outside TurnSteps remain directly visible without aggregate headers
- **AND** independent Reasoning and subagent events remain in their established transcript positions

### Requirement: Single tools render directly
The transcript SHALL render a single tool without an additional group summary, preserving access to invocation, output and diff details. Groups containing multiple ordinary tools SHALL also render their calls directly without aggregate disclosures.

#### Scenario: One tool between narrative events
Test: unit — transcript projection and group-disclosure policy; native render checked visually.
- **WHEN** a streaming or completed turn contains a group with one ordinary tool
- **THEN** the tool row is visible without a redundant group header
- **AND** its details remain independently expandable

#### Scenario: Several consecutive tools
Test: unit — transcript group-disclosure policy.
- **WHEN** a group contains multiple ordinary tools
- **THEN** every call is directly visible and only the completed TurnSteps summary displays aggregate counts

### Requirement: Compact consistent event typography
The transcript SHALL use compact event rows and regular sans typography consistent with narrative text. Command headers SHALL use Ran command or Running command and a bounded summary in a quieter tone. Full invocation and output SHALL remain monospaced in the expanded payload. Failure colors SHALL remain semantic. Expanded command headers and payloads SHALL share a single frame. Reasoning bodies SHALL have an inset left rule distinct from narrative.

#### Scenario: Mixed narrative and tool activity
Test: unit — command projection and full invocation retention; none — native GPUI visual acceptance.
- **WHEN** a turn contains long or compound commands and open reasoning
- **THEN** command headers show bounded sans summaries, with action stronger than detail
- **AND** expanding a command reveals its retained invocation and output inside the header frame
- **AND** reasoning has a left rule, respects explicit disclosure choices and shows the working dot spinner only while active

### Requirement: Operational events default to compact summaries
The transcript SHALL start tool payloads closed and reasoning disclosures open, including during streaming and inside expanded turn steps. It SHALL preserve explicit user disclosure choices and expose all recorded details by expansion. Reasoning SHALL use the fixed label Thinking while active and Thought when complete. Its content SHALL appear in the body, open by default with explicit user collapse preserved. The reasoning arrow SHALL remain visible at rest.

#### Scenario: Interleaved reasoning and tools
Test: unit — transcript disclosure defaults and content preview; native render acceptance.
- **WHEN** a turn contains repeated reasoning and ordinary multi-tool groups
- **THEN** individual tool rows display directly without group counters, while tool payloads stay closed and reasoning details start open
- **AND** opening an event reveals its existing detail content
- **AND** explicit fold choices remain authoritative

### Requirement: Disclosure arrows follow event text
The transcript SHALL place event disclosure arrows immediately after their content, using text-width labels that can shrink and truncate in constrained columns. JavaScript and Python eval rows SHALL omit a redundant language prefix when their icon already identifies the language.

#### Scenario: Short and long event labels
Test: none — native GPUI visual acceptance.
- **WHEN** a tool, reasoning, task or turn summary has an expansion arrow
- **THEN** the arrow follows its text instead of occupying the far column edge
- **AND** long labels truncate without clipping the arrow

#### Scenario: JavaScript eval has a title
Test: unit — stream_copy projection.
- **WHEN** a JavaScript or Python eval tool has a descriptive title
- **THEN** the row shows the title without a duplicate language prefix
- **AND** non-eval text and unrecognized language labels remain intact

### Requirement: Expanded tool payloads use a code surface
The transcript SHALL render an expanded tool's invocation and result inside one bordered, rounded, monospaced surface with bounded height and internal scrolling. Long lines SHALL remain available through horizontal scrolling, and existing diff semantics and explicit truncation notices SHALL be preserved.

#### Scenario: A command has long output
Test: unit — payload height budget; native GPUI scroll acceptance.
- **WHEN** the expanded invocation and result exceed the code viewport
- **THEN** the transcript allocates a bounded height and the body scrolls internally
- **AND** invocation and result remain inside the same surface

#### Scenario: A short invocation has a short result
Test: unit — payload height budget; native GPUI visual acceptance.
- **WHEN** the combined content fits the code viewport
- **THEN** the surface fits its content without an empty fixed-height area

### Requirement: Disclosure arrows appear on row hover
Tool and turn disclosure arrows SHALL remain hidden at rest and appear only while their own header row is hovered. Reasoning arrows SHALL stay visible at rest. Their space and position after the text SHALL remain stable in both states.

#### Scenario: Hover one event
Test: native GPUI visual acceptance.
- **WHEN** the pointer enters a tool, tool group, task or turn summary row
- **THEN** that row's disclosure arrow becomes visible without shifting its text
- **AND** other hover-only rows' arrows remain hidden and reasoning arrows remain visible

### Requirement: Specialized operational output stays consistent and inspectable
The transcript SHALL apply the compact event typography and inline hover disclosure contract to file edits, commands, searches, MCP calls, generic calls, tasks and reasoning. Expanded payloads SHALL use bounded code or diff surfaces, and failures SHALL retain their recorded details even when specialized previews are absent.

#### Scenario: Failed specialized tool
Test: unit — projection and height; none — native GPUI visual acceptance.
- **WHEN** a file or task tool fails with a recorded diagnostic
- **THEN** its diagnostic remains available by expansion
- **AND** its header indicates failure using the common event presentation

#### Scenario: Mixed specialized output
Test: none — native GPUI visual acceptance; unit for labels and height budgets.
- **WHEN** the user expands command, MCP, search, edit, task and reasoning output
- **THEN** headers share typography and inline hover-only arrows
- **AND** code and diffs retain syntax and internal scrolling without changing execution state

### Requirement: Repeated presentation within a turn is suppressed
The transcript SHALL display an identical standalone error or image path only once within an assistant entry. Successful task snapshots with no changed items SHALL not create empty disclosures. Distinct errors, images, failed task calls and subsequent turns SHALL remain visible.

#### Scenario: Tool and text repeat an image
Test: unit — transcript projection.
- **WHEN** a tool and narrative text reference the same image path within an entry
- **THEN** one inline image is displayed
- **AND** a different path or later entry remains independently visible

#### Scenario: Error and terminal error repeat
Test: unit — transcript projection.
- **WHEN** two standalone error parts have the same diagnostic in an entry
- **THEN** one error presentation is displayed
- **AND** a distinct diagnostic remains visible

#### Scenario: Unchanged successful task list
Test: unit — task projection.
- **WHEN** a successful task update repeats the previous list
- **THEN** it creates no empty task disclosure
- **AND** duplicate task titles do not hide changes to separate occurrences

### Requirement: Native content presentation preserves semantics
The transcript SHALL present narrative, code, user attachments and subagent summaries using consistent typography while preserving native links, previews, real child status and source data. Any nonempty reasoning body SHALL remain expandable, including a short plain paragraph or text equal to the fixed state label.

#### Scenario: Reasoning contains only its title
Test: unit — reasoning body policy; none — native GPUI visual acceptance.
- **WHEN** reasoning contains one plain paragraph
- **THEN** its header shows only Thinking or Thought and the paragraph remains available through expansion
- **AND** longer or structured reasoning remains expandable

### Requirement: Activity and narrative retain visual hierarchy
Expanded turn activity SHALL preserve the transcript spacing between narrative blocks and operational groups. Consecutive operational rows SHALL remain compact. File details SHALL use an integrated 28px header and unified diff card with old/new line gutters, a preview bounded to 260px and explicit expansion and mixed-tool summaries SHALL use a neutral activity icon. Failed calls SHALL not be labelled as successful creation or execution.

#### Scenario: User opens completed activity
Test: unit — narrative boundary gaps, file preview heights and unified line gutters; none — native GPUI visual acceptance.
- **WHEN** the user opens a completed turn containing narrative, tools and file changes
- **THEN** context changes have visible spacing while tool rows stay compact
- **AND** file previews remain bounded until expanded, with header and code in one frame and no fabricated line positions for truncated tails
- **AND** the activity block is visually distinct from the final response

### Requirement: Reasoning uses the working spinner
Active reasoning SHALL reuse the same native 3×3 gradient dot spinner as the working indicator, including its 2.5px cell size, colors and shared animation clock. It SHALL disappear completely when reasoning completes and SHALL NOT add a second spinner. Existing disclosure behavior SHALL remain unchanged.

#### Scenario: Thinking settles
Test: none — native visual acceptance of the existing shared spinner.
- **WHEN** active reasoning completes
- **THEN** the label becomes Thought and the spinner is removed, leaving no SVG/icon
- **AND** the row keeps its alignment and expandable content

### Requirement: Final answer is separated from completed activity
The transcript SHALL display one quiet horizontal rule across the content column between completed turn activity and its final answer. The rule SHALL remain below all activity when turn steps are expanded and below the summary when collapsed. It SHALL NOT introduce rules between intermediate commentary and tools or within the final answer.

#### Scenario: Toggle completed activity
Test: unit — existing completed turn projection; none — native GPUI separator appearance.
- **WHEN** a completed turn contains operational activity followed by a final answer
- **THEN** one separator appears after the activity and before the answer
- **AND** expanding or collapsing activity preserves that boundary without duplicating the separator

### Requirement: Generic tools use their names directly
Generic Unknown tool headers without a specialized presenter SHALL display their icon and tool name without Ran tool or Running tool prefixes. The name SHALL retain truncation and the primary neutral header tone. Question tools SHALL use the interactive question history presenter instead. Eval headers SHALL display their title or tool name without Evaluated/Evaluating; MCP headers SHALL display server and tool without Called tool/Calling tool. Other specific semantic tool labels, failure presentation, active indicators and expanded payloads SHALL remain available.

#### Scenario: Generic tool settles
Test: unit — native stream copy projection.
- **WHEN** a generic tool without a specialized presenter is pending or completes
- **THEN** its normal header text is only its tool name
- **AND** eval and MCP headers omit execution verbs in both lifecycle states, while hub and command headers keep their semantic labels
- **AND** question tools render their question lifecycle and answers instead of an ask header

#### Scenario: Generic ask settles
Test: unit — native question projection.
- **WHEN** an ask tool is pending or completes
- **THEN** its specialized presenter displays Asking question, Waiting for response, or Answer/Answers as appropriate
- **AND** neither ask nor Ran tool is displayed as a generic header

### Requirement: Interactive question history
The transcript SHALL present interactive question requests as question lifecycle content, never as a generic ask tool payload. Submitted answers SHALL survive document serialization and reload.

#### Scenario: Submitted answers
- **WHEN** a request is answered
- **THEN** the transcript displays a single Answer or Answers card with questions in original order and selected labels matched by question id
- **Test:** unit

#### Scenario: Pending questions
- **WHEN** a question call starts and its interactive panel becomes available
- **THEN** the transcript progresses from Asking question to the question and Waiting for response, with controls only above the composer
- **Test:** unit

#### Scenario: Legacy or skipped request
- **WHEN** stored answer data is absent or the request is skipped
- **THEN** the UI states that answers are unavailable or the question was skipped without inventing an answer
- **Test:** unit

#### Scenario: Native answer presentation
- **WHEN** answered questions are rendered
- **THEN** a rounded bordered card has a compact icon header and regular sans answer text below each brighter question, without duplicate ask headers
- **Test:** none — native screenshot verification

### Requirement: Tool code blocks wrap within the transcript
Expanded invocation and output code blocks SHALL wrap at the available card width without horizontal scrolling. Their height SHALL follow wrapped text up to the vertical viewport limit, and adjacent expanded cards SHALL have an 8px separation. Source whitespace, syntax highlighting and copy content SHALL remain intact.

#### Scenario: Long command and output
Test: none — native GPUI visual verification.
- **WHEN** a command or output line exceeds the available width
- **THEN** it wraps inside its code block and cannot shift the output sideways
- **AND** long content remains vertically scrollable

#### Scenario: Consecutive expanded cards
Test: none — native GPUI visual verification.
- **WHEN** two tool code blocks are expanded consecutively
- **THEN** an 8px gap separates their borders

### Requirement: Working indicator sits above the composer
The main Chat working indicator SHALL occupy the existing reserved status strip directly above the composer, aligned with the input column. It SHALL remain outside transcript scrolling and SHALL NOT also appear in the transcript rows. The transcript SHALL retain the former in-flow indicator footprint as blank space so docking moves only the indicator, not the last content. This reservation SHALL NOT mount a second spinner. The existing elapsed time, active spinner, sending, queued and retry states SHALL remain intact. Subagent panes SHALL retain their independent transcript indicator.

#### Scenario: Live reply grows or scrolls
Test: none — native GPUI positioning and existing presenter reuse.
- **WHEN** a Chat is working and its transcript grows or scrolls
- **THEN** its indicator remains directly above the composer with a small gap
- **AND** there is only one main Chat working indicator
- **AND** the last content keeps its previous spacing above the input

### Requirement: File diff cards wrap within their viewport
Write and Edit cards SHALL wrap long text within the available code column without horizontal scrolling. Old/new line numbers and diff signs SHALL remain aligned with the first visual line, and the change background SHALL cover the full width and height of each logical line. Number gutters SHALL fit the digits present and omit unused sides; truncated tails without reliable numbers SHALL NOT reserve empty number columns. Wrapping SHALL preserve source text, whitespace, syntax and change counts. Small previews SHALL size naturally up to their existing height cap; large fetched previews SHALL retain virtualization with variable row heights. Resizing SHALL remeasure wrapped rows. Vertical scrolling, lazy loading and rounded corners SHALL remain intact.

#### Scenario: Inspect long file changes
Test: none — native GPUI wrapping and variable-height list; unit — existing preview, gutter and height contracts.
- **WHEN** a user reads or expands a file change containing long indented lines
- **THEN** the entire line is available through wrapping and vertical scrolling without lateral scrolling
- **AND** continuation text stays in the code column, with full-width addition/removal backgrounds and no repeated logical line numbers
- **AND** large previews remain virtualized after resizing

### Requirement: Tool headers identify the invocation without redundant execution copy
Skill headers SHALL show the invoked skill identifier beside Skill when supplied in the invocation. Hub headers SHALL show the operation and target detail without Ran hub or Running hub. Missing identifiers SHALL retain a truthful tool-name fallback. Error presentation and expandable payloads SHALL remain available. Rust and edge render sanitizers SHALL retain only the skill identifier keys (`skill`, `path`, `name`) and drop other Skill input fields, idempotently.

#### Scenario: Inspect skill and hub calls
Test: unit — native stream copy in transcript.rs and Rust/edge render input sanitizer.
- **WHEN** a skill or hub invocation is active or completed
- **THEN** its header names the supplied skill or specific hub operation and target

### Requirement: File cards have no separate expansion footer
Write/Edit cards SHALL end at their diff viewport without a reserved empty expansion band. The existing header SHALL retain expansion and collapse, lazy loading and rounded clipping.

#### Scenario: Inspect expanded file card
Test: none — native GPUI visual review.
- **WHEN** a user expands or collapses a file card
- **THEN** no empty footer is reserved beneath its content viewport
- **AND** header expansion and vertical scrolling remain available

### Requirement: Sibling subagents share one visual row
Multiple adjacent subagents belonging to the same recorded task invocation SHALL render individual clickable pairs of a larger (28px) avatar followed immediately by its own name in one wrapping row with a shared lifecycle summary. Different task invocations and unassociated single spawns SHALL remain separate. Each avatar and name SHALL open its own subagent transcript. Running and failed children SHALL remain distinguishable; completing the parent SHALL NOT mark running children completed. Per-child identities and aggregate activity counts SHALL remain unchanged.

#### Scenario: Fan out several children
Test: unit — transcript projection; none — native GPUI avatars, wrapping and clicks.
- **WHEN** a task invocation starts two or more linked subagents
- **THEN** their avatars and names share a row
- **AND** each child remains independently navigable
- **AND** another invocation is not merged into that row

### Requirement: Preview identity and file card surfaces match their source
Subagent preview tabs SHALL show the same doc-keyed avatar as their transcript link, with activity shown separately. File change cards SHALL use the same neutral background as command cards, retaining semantic diff washes and rounded clipping.

#### Scenario: Open a subagent preview
Test: none — native GPUI visual validation.
- **WHEN** the user opens a subagent
- **THEN** its preview tab displays that subagent's avatar

#### Scenario: Render an edit card
Test: none — native GPUI visual validation.
- **WHEN** a file edit is rendered beside command cards
- **THEN** their neutral backgrounds match without a black fill

### Requirement: File edits show a bounded live typing preview
Write/Edit cards SHALL display progressive generated content in a 72px collapsed viewport, with at most 15 recent lines, bottom aligned after three lines. Text SHALL wrap with no horizontal scrolling. Partial input refreshes SHALL be time gated at 100ms after initial semantic previews, preserving bounded decoding and final content delivery. Active cards SHALL omit syntax highlighting and show filename shimmer and a spinner. Completion SHALL replace activity with line statistics and expansion affordances, requesting syntax highlighting after 50ms. Header and collapsed body SHALL expand the final file up to 200px with vertical scrolling. Errors SHALL remain visible.

#### Scenario: Small incremental input chunks
Test: unit — harness partial input decoder.
- **WHEN** a file tool streams small chunks beyond the initial preview
- **THEN** its preview refreshes after 100ms without requiring 16KB of new input
- **AND** final flush preserves the last decoded content

#### Scenario: Compact generated tail
Test: unit — ui file_change projection; none — native GPUI layout and shimmer.
- **WHEN** Write or Edit generates more than three lines
- **THEN** at most 15 generated lines are bottom aligned in a 72px viewport
- **AND** completion restores the authoritative diff and permits expansion to 200px

### Requirement: File card layout work remains bounded and preserves the current sticky turn
Collapsed file cards SHALL use only the bounded durable preview even when full input has been fetched. Reopening SHALL reuse cached full input. Measuring a card SHALL NOT continuously schedule frames and SHALL invalidate only subsequent user-row geometry when its height changes; the current turn header SHALL remain available.
#### Scenario: Collapse after loading a large file
Test: unit — ui file_change preview selection; none — native GPUI interaction and frame scheduling.
- **WHEN** a large fetched file is collapsed and reopened
- **THEN** collapsed rendering uses the bounded preview and reopening uses the cached full file
- **AND** unchanged measurements schedule no further render
#### Scenario: A file below the sticky header changes height
Test: unit — sticky geometry retention; none — native streaming review.
- **WHEN** a file card below the current user row changes height
- **THEN** its current sticky geometry remains valid and only subsequent user rows are invalidated

### Requirement: Sticky geometry uses a consistent layout coordinate system
The sticky turn SHALL use a scroll offset within the laid-out viewport range. Recorded user positions and scroll offsets SHALL come from the same completed layout. Pending end anchors during streaming SHALL NOT switch the header to another turn.
#### Scenario: Streaming while anchored at the end
Test: unit — offset projection; none — native multi-turn streaming.
- **WHEN** a stream chunk sets a past-end list anchor before layout
- **THEN** the sticky projection uses the valid scroll range and retains the current turn

### Requirement: Tool details close without waiting for another event
A tool detail using natural wrapped height SHALL unmount its body immediately when closed and SHALL NOT retain it for an animation that is not running.
#### Scenario: Close a completed tool detail
Test: none — native GPUI interaction.
- **WHEN** the user closes an expanded tool detail after streaming has stopped
- **THEN** the next rendered frame shows the closed detail without requiring a timer, pointer movement or a new stream event

### Requirement: Streaming preserves active interactions
Streaming SHALL suspend automatic following when text selection starts and SHALL preserve the selected anchor. Repaints with unchanged input SHALL preserve composer geometry and text, including IME and mention decorations. Returning from a closed pane SHALL restore keyboard handling to a mounted control without stealing focus from another mounted editor.

#### Scenario: Select while the response grows
Test: unit — selection follow transition; none — native streaming selection acceptance.
- **WHEN** the user presses and drags to select text during streaming
- **THEN** automatic scrolling stops before further stream updates can displace the anchor

#### Scenario: Repaint and return from a preview
Test: unit — layout cache invalidation; none — native preview, focus and typing acceptance.
- **WHEN** a preview is closed while the Chat is streaming
- **THEN** the composer and shortcuts respond on the first interaction
- **AND** unchanged input does not oscillate between layouts
