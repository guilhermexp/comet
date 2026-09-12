## ADDED Requirements

### Requirement: Gray accent distinguishes inline code with a background

With the explicit Gray accent selected, native Markdown SHALL paint a subtle rounded theme background behind inline code in light and dark appearances. Colored accents SHALL keep their existing presentation. This decoration SHALL preserve text, wrapping, selection and link targets and SHALL NOT apply to surrounding prose or fenced code.

#### Scenario: Switch between Gray and a colored accent

Test: unit — Markdown run styles across accent presets and appearances; native visual QA for rounded geometry.

- **WHEN** the user selects Gray
- **THEN** inline code receives the themed background
- **AND** selecting another accent removes that background without changing the text or links
