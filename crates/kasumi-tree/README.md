# kasumi-tree

分代节点 arena、reconcile / commit、命中测试与无障碍快照。

## Frame pipeline

1. The backend converts winit events into an `InputState` and computes the
   device scale and logical viewport.
2. `Runtime::frame` processes input against the previous layout. Hover, active
   and focus targets are resolved here. Hit testing against the previous frame
   is deliberate: a widget's current size is not known until `view` returns.
3. `view` runs with a `Ui`. Every declaration is reconciled by id into the
   retained tree. A new id allocates a node; a matching id reuses it.
4. `Tree::end_frame` commits child order, recycles nodes that were not touched
   and ends the state TTL window.
5. `LayoutEngine` updates the changed parts of its taffy mirror and runs layout.
   Absolute rectangles are written back to the nodes.
6. The paint pass walks the committed tree and appends rects and positioned
   glyphs to one `DrawList`, preserving z-order.
7. The backend uploads the draw list as one instance buffer and renders it with
   one shader. A frame is requested again only for animations, explicit
   repaint requests or input that changes what is on screen.

## Identity, state and interaction

### Identity and reconciliation

The `Ui` holds an id scope and a sibling counter. A child id is
`mix(parent_scope, sibling_index)`. Containers become the scope for their
children. `ui.keyed(key, |ui| ...)` replaces the sibling-derived scope with
`hash(parent_scope, key)`, so reordering keyed items does not reparent their
subtrees.

State ids use a separate namespace. Adding or removing a `ui.state` call does
not shift widget ids.

Every declared node id is checked against a per-frame table that records which
widget declared it. A duplicate panics with the id, the frame number and both
origins. Duplicate ids are normally caused by an unkeyed conditional branch or
by reordering unkeyed siblings; wrapping the branch in `ui.keyed` fixes both
cases.

### State and interaction

`ui.state(|| value)` returns a cloneable `State<T>`. Handles can be stored on
application structs or captured by closures; they do not borrow the `Ui`. The
API is `get()` (requires `Clone`), `with()` for a zero-copy read, `set()` and
`update()`. All writes request a repaint. Since a write can happen after the
widgets that read the value were declared, a write schedules one follow-up
frame; that frame is also the point at which the rendered result observes the
new value. State ids use a namespace separate from node ids, so adding or
removing a state call does not shift widget ids.

Values whose `ui.state` call stops running for 600 frames are evicted.
Declaring a slot keeps it alive, so a value that is declared every frame but
read only occasionally (a draft, an id allocator) holds its value: eviction
follows declaration, not reads. `ui.state_persistent` opts a value out of
eviction; `Runtime::set_state_ttl_frames` changes the window. An evicted handle
re-runs its initializer on the next access.

Widgets return `Response`, which reports `hovered`, `pressed`, `clicked`,
`released`, `dragged`, `context_clicked`, `focused`, `changed` and the current
scroll offset. Hover and click resolution uses the previous frame's
rectangles, as described above. Tab order is the depth-first order of
focusable nodes in the current tree. Enter and Space activate focused buttons,
checkboxes, radios and combo boxes.

The text field keeps caret, selection and undo state on its retained node while
the text itself remains a `State<String>`. It supports Shift+arrows,
Ctrl/Cmd+arrows, Home/End, Delete/Backspace, Ctrl/Cmd+Z, Ctrl/Cmd+Shift+Z,
Ctrl/Cmd+A/C/X/V, mouse selection and IME composition. The backend owns the
system clipboard integration.

`Runtime::a11y_snapshot` produces a flat list of roles, labels, bounds, depth
and focus state from the retained tree. It is a data API only; there is no
platform integration yet.

See also: [`docs/ARCHITECTURE.md`](../../docs/ARCHITECTURE.md) · [`docs/GUIDE.md`](../../docs/GUIDE.md).
