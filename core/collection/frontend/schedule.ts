// A schedule's rules as the Collectors page edits them and draws their week, for another schedule
// whose rules run the same way (`collection/backend/rules.rs`): a DSP's backups.
import './collectors/collectors.css';

export { RuleEditor } from './collectors/RuleEditor.js';
export { WeekPreview, runDay, until } from './collectors/WeekPreview.js';
export {
  EVERY_DAY,
  finished,
  ruleText,
  type Editing,
  type RuleTiming,
} from './collectors/rules.js';
