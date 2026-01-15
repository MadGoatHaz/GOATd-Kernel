# **Egui Architect’s Reference: A Comprehensive Guide to Immediate Mode Interface Design**

## **1\. Introduction: The Immediate Mode Paradigm and Egui Philosophy**

The architectural landscape of graphical user interfaces has long been dominated by the retained mode paradigm, where the application maintains a persistent tree of widget objects—DOM elements, QWidgets, or view hierarchies—that statefully exist in memory between frames. Egui fundamentally rejects this model in favor of Immediate Mode GUI (IMGUI), a paradigm that aligns interface construction directly with the application’s execution flow. In this model, the interface is not a static structure stored in RAM but a transient byproduct of code execution. The user interface is rebuilt from scratch every single frame, typically at a rate of 60 times per second.1

This architectural decision has profound implications for the developer. It eliminates the need for synchronization between application state and UI state, a notorious source of bugs in retained mode systems. In egui, the UI *is* a direct reflection of the application state at that specific millisecond. If a variable counter exists in your struct, the UI displays it immediately because the code to draw the label ui.label(counter.to\_string()) is executed freshly every frame.1 There are no callbacks to attach, no listeners to register, and no stale view objects to invalidate. The widget typically returns a Response struct immediately upon creation, indicating whether it was clicked, hovered, or dragged during that specific frame, allowing logic to be handled inline: if ui.button("Save").clicked() { save(); }.2

However, this simplicity imposes a strict discipline on the developer. Because the entire UI logic runs every frame, the layout code must be highly performant. Expensive operations cannot block the UI thread, as this would freeze the rendering. State that must persist across frames—such as scroll positions, window locations, or the text inside an input field—cannot be stored in the widgets themselves, as the widgets are destroyed and recreated milliseconds later. Instead, egui manages this via a specialized Memory system that tracks persistent state using unique identifiers (Id), automatically handling the bridge between the transient frame logic and the user's continuity of experience.3 This document serves as a comprehensive, dense reference manual for navigating this architecture, detailing the mechanisms of layout, input, styling, and state management that define professional egui development.

## ---

**2\. Core Architecture: Context, Lifecycle, and Integration**

The foundation of any egui application lies in its integration with the host platform and the central management of the UI lifecycle. Unlike a framework that controls the main loop entirely, egui is a library that is typically embedded within a host shell—most commonly eframe for desktop and web, though it can be integrated into game engines like Bevy or bare-bones renderers like wgpu.1

### **2.1 The Context (egui::Context)**

The Context is the "god object" of the library, acting as the centralized repository for all global state. It transcends the frame boundary, persisting input states, memory caches, style definitions, and texture atlases. It is designed to be thread-safe and cheap to clone, utilizing internal reference counting and interior mutability via RwLock to manage concurrent access.5

**Deadlock Hazards:** The reliance on internal locking mechanisms introduces a critical constraint: developers must strictly avoid recursive locking. Attempting to acquire a read lock on the context (e.g., via ctx.input()) inside a closure that already holds a lock (e.g., another ctx.input() block) will result in an immediate deadlock. All data access must be transactional and short-lived.5

#### **2.1.1 The Rendering Loop and Repainting**

The Context drives the rendering loop through a series of request methods. Egui is reactive; it does not redraw unless necessary. However, because it is immediate mode, "necessary" often means "continuous" during animations or interactions.

* **request\_repaint()**: This is the primary signal to the backend that the interface has changed or an animation is active. It ensures a new frame is generated immediately after the current one concludes. This is essential when background threads finish loading data or when external events (like a network packet) occur.5  
* **request\_repaint\_after(Duration)**: This method allows for throttled updates, useful for lower-frequency polling or pacing animations to avoid consuming 100% of a CPU core. For instance, a blinking cursor might request a repaint every 500ms.5  
* **request\_discard()**: In rare cases, such as the first frame of a complex grid layout where cell sizes are unknown, the layout engine may produce visual artifacts. This method instructs the backend to throw away the current frame's geometry and immediately re-run the logic, allowing the layout to stabilize before being presented to the user. This supports "multi-pass" immediate mode layouts.2

### **2.2 Integration via eframe::App**

For standalone applications, the eframe crate provides the native windowing shell. The developer implements the App trait, which defines the application's lifecycle hooks.

#### **2.2.1 The Update Loop (update)**

The update method is the core of the application. Its signature, fn update(\&mut self, ctx: \&egui::Context, frame: \&mut eframe::Frame), provides mutable access to the application state (self) and the UI context. This method is called every time the OS or the Context requests a repaint. It is within this method that the entire UI tree—panels, windows, widgets—is defined procedurally.6

#### **2.2.2 Persistence and State Preservation**

Egui provides a robust system for saving application state (e.g., user preferences, window positions) across sessions.

* **save Hook**: The save(\&mut self, storage: \&mut dyn Storage) method is called on shutdown and periodically (controlled by auto\_save\_interval). The developer is responsible for serializing their struct (typically using Serde) and writing it to the abstract storage backend, which maps to local file systems on desktop or Local Storage on the web.6  
* **persist\_egui\_memory**: By returning true in this hook, the developer instructs eframe to automatically save the internal egui::Memory to disk. This preserves transient UI details like which collapsing headers are open, scroll positions, and window sizes, ensuring the user returns to the exact same visual state.6

#### **2.2.3 Raw Input Hooks**

The raw\_input\_hook method allows developers to intercept operating system events before they reach the egui processing pipeline. This is critical for implementing global shortcuts that should function regardless of focus (e.g., a "toggle overlay" key in a game) or for filtering out inputs that the UI should ignore. It provides mutable access to the RawInput struct, allowing events to be consumed or modified.6

## ---

**3\. The Layout Engine: Geometry and Containers**

The layout engine in egui is responsible for distributing screen real estate among widgets. Unlike flexbox or grid systems in web development which solve constraints globally, egui's layout is predominantly single-pass and recursive. The Ui struct tracks a cursor (a Rect) representing the available space and advances it as widgets are added.

### **3.1 The Ui Struct and Space Negotiation**

The Ui struct represents a specific region of the screen with a defined layout direction (e.g., Left-to-Right or Top-to-Bottom). Every widget added to a Ui engages in a negotiation process:

1. **Request:** The widget asks for a desired size (e.g., a button based on its text width).  
2. **Allocation:** The Ui checks its available\_size() and allocates a Rect for the widget via allocate\_space.  
3. **Placement:** The Ui returns a Response containing the assigned Rect, and advances its internal cursor to the next available position.7

The Ui maintains two critical bounding boxes:

* **min\_rect**: The rectangle encompassing all widgets *currently* placed in the UI. This grows as widgets are added.  
* **max\_rect**: The maximum bounds allowed for this UI (e.g., the size of the window or panel). Layout operations use this to determine wrapping or clipping behavior.7

### **3.2 Layout Primitives**

Layouts are defined using closures that create a new child Ui with specific allocation rules.

| Layout Method | Direction | Alignment (Cross-Axis) | Usage Context | Source |
| :---- | :---- | :---- | :---- | :---- |
| horizontal | Left $\\to$ Right | Center (Y-axis) | Standard button rows, toolbars. | 7 |
| vertical | Top $\\to$ Bottom | Left (X-axis) | Forms, lists, sidebars. | 7 |
| horizontal\_top | Left $\\to$ Right | Top (Y-axis) | Aligning items of varying height to the top. | 7 |
| horizontal\_wrapped | Left $\\to$ Right (Wrap) | Center (Y-axis) | Tag clouds, file explorers. Wraps at max\_width. | 7 |
| vertical\_centered | Top $\\to$ Bottom | Center (X-axis) | Splash screens, centered menus. | 7 |
| columns | N/A | N/A | Divides width into $N$ equal vertical strips. | 7 |

**Scope and Isolation:** The ui.scope(|ui|...) method creates a temporary child Ui that shares the parent's cursor but allows for isolated modifications to style or visuals. Any changes made to ui.style\_mut() inside the scope are reverted once the closure exits, making it ideal for localized theming (e.g., making one section of text red without affecting the global style).7

### **3.3 Root Containers**

Containers are the top-level elements that define the structure of the application window.

#### **3.3.1 Panels**

Panels dock to the edges of the screen and subtract their size from the available area for subsequent panels. Order is strictly enforced: the first panel added is the outermost.

* **SidePanel**: Docks to the left or right. It is resizable by default, allowing users to drag the edge to expand the panel. Usage: SidePanel::left("id").show(ctx, |ui|...).8  
* **TopBottomPanel**: Docks to the top or bottom (e.g., menu bars, status bars).  
* **CentralPanel**: This must be added *last*. It consumes all remaining space in the center of the window. If no central panel is added, the background remains transparent or uses the clear color.8

#### **3.3.2 Windows (egui::Window)**

Windows are floating, overlapping containers that manage their own position and size via Memory.

* **Capabilities:** By default, windows include a title bar, are draggable, resizable, and have a drop shadow.  
* **Configuration:**  
  * resizable(bool): Toggles the resize corner handle.  
  * vscroll(bool): Enables internal scrolling if content exceeds height.  
  * collapsible(bool): Adds a collapse chevron to the title bar.  
  * anchor(Align2, Vec2): Sets the initial position relative to the screen (e.g., top-right corner).  
  * fixed\_size(Vec2): Disables resizing and forces dimensions.9

#### **3.3.3 Grid Layout (egui::Grid)**

The Grid layout is distinct from columns in that it aligns content in both rows and columns dynamically. It is useful for property editors or forms.

* **Mechanism:** It operates as a state machine. You must call ui.end\_row() to signal the completion of a row.  
* **Sizing:** Column widths are calculated based on the widest element in that column.  
* **Styling:**  
  * striped(bool): Adds alternating background colors for readability.  
  * num\_columns(usize): Hints the column count; the last column is often treated as "filling" the remaining width.  
  * spacing(Vec2): Customizes the gap between cells.10

#### **3.3.4 Areas (egui::Area)**

An Area is a low-level container that is positionable (movable) but lacks the decorations (title bar, borders) of a Window. It is typically used for notifications, tooltips, or custom floating overlays that sit on top of other content. It persists its position in Memory.9

## ---

**4\. Input Handling System**

In immediate mode, input handling differs significantly from event-driven systems. There are no listeners. Instead, the Context aggregates input from the OS each frame into an InputState, and widgets query this state during their execution to determine if they are being interacted with.

### **4.1 The InputState**

The InputState struct, accessed via ctx.input(|i|...), provides a snapshot of all input devices for the current frame.

#### **4.1.1 Keyboard Input**

Keyboard state is queried using the Key enum, which abstracts physical keys into logical constants.11

* **Press vs. Down:** i.key\_pressed(Key::A) returns true only on the frame the key was initially pressed. i.key\_down(Key::A) returns true as long as the key is held.  
* **Modifiers:** The i.modifiers field (containing .ctrl, .shift, .alt, .command) allows for checking combinatory inputs. Example: if i.modifiers.ctrl && i.key\_pressed(Key::S) { save(); }.11  
* **Event Consumption:** To prevent an event from bubbling (e.g., if a widget consumes the "Enter" key), one would typically use ctx.input\_mut() or handle the event via the widget's Response to mark interaction, though explicit consumption is often handled internally by widgets like TextEdit.11

#### **4.1.2 Pointer Input**

The PointerState tracks mouse and touch interactions seamlessly.

* **Position:** i.pointer.hover\_pos() gives the current cursor coordinates. i.pointer.latest\_pos() gives the last known position (useful if the mouse left the window).5  
* **Buttons:** PointerButton enum covers Primary (Left), Secondary (Right), and Middle.  
* **Gestures:** The input state also aggregates high-level gestures:  
  * zoom\_delta(): Pinch-to-zoom factor.  
  * scroll\_delta: 2D scrolling vector (touchpad or wheel).11

### **4.2 The Response Struct**

Whenever a widget is added to the Ui, it returns a Response struct. This object acts as the bridge between the widget's geometric definition and the user's interaction. It aggregates the intersection of the input state with the widget's rectangle.12

#### **4.2.1 Interaction Logic**

* **Clicks:** .clicked() is the standard check for a primary click. .secondary\_clicked() detects right-clicks. .double\_clicked() handles rapid consecutive clicks.  
* **Hover:** .hovered() indicates the mouse is strictly over the widget and not obstructed by a popup. .contains\_pointer() is a broader check that ignores whether the widget is blocked or disabled.  
* **Drags:** .dragged() returns true while the user holds the primary button and moves the mouse. .drag\_delta() returns the Vec2 movement vector for that frame, allowing for easy implementation of sliding or moving logic.12

#### **4.2.2 Logic Combinators**

Response objects can be combined using the bitwise OR operator (|). This is useful when a logical "widget" is composed of multiple egui primitives.

* **Example:** let resp \= ui.button("A") | ui.button("B"); if resp.clicked() {... }. The result is true if *either* button was clicked.12

#### **4.2.3 Focus Management**

Widgets that accept text input (like TextEdit) interact with the focus system.

* has\_focus(): True if the widget owns the keyboard.  
* request\_focus(): Programmatically forces focus to this widget.  
* surrender\_focus(): Releases focus.  
* lost\_focus() / gained\_focus(): Edge triggers for detecting focus transitions.12

## ---

**5\. The Widget Ecosystem: Builders and Definitions**

Standard widgets in egui follow the Builder Pattern. They are initialized with their mutable state (if any) and configured via method chaining before being added to the Ui.

### **5.1 Button**

A fundamental interactive element.

* **Constructor:** Button::new(text).13  
* **Configuration:**  
  * shortcut\_text(str): Aligns shortcut text (e.g., "Ctrl+O") to the right.  
  * fill(Color32) / stroke(Stroke): Customizes the visual style explicitly.  
  * frame(bool): Toggles the background/border rendering.  
  * small(): Compresses padding for embedding in dense text.  
  * selected(bool): Renders the button in a "pressed" or "highlighted" state, useful for toggle interfaces.

### **5.2 Label and Hyperlink**

* **Label:** Static text. Label::new(text). Can accept RichText for styling (color, size, bold).  
* **Hyperlink:** Clickable text. Hyperlink::new(url). Use open\_in\_new\_tab(bool) to control browser behavior.14

### **5.3 Boolean Controls: Checkbox and RadioButton**

* **Checkbox:** Checkbox::new(\&mut bool, label). The boolean reference is updated automatically when clicked. Supports .indeterminate(bool) to show a "dash" state visually, though the logic remains binary.15  
* **RadioButton:** RadioButton::new(checked: bool, label). Unlike Checkbox, this does *not* take a mutable reference. Usage involves logic: if ui.add(RadioButton::new(val \== Enum::A, "A")).clicked() { val \= Enum::A; }.

### **5.4 Numeric Controls: Slider and DragValue**

* **Slider:** A bar for selecting a value within a range.  
  * Slider::new(\&mut value, range).  
  * .logarithmic(true): Essential for wide ranges (e.g., frequency).  
  * .clamp\_to\_range(bool): Determines if values outside the range are forced in.  
  * .step\_by(f64): Enforces discrete steps.  
  * .orientation(Vertical): Renders a vertical slider.16  
* **DragValue:** A number display that can be dragged to change.  
  * DragValue::new(\&mut value).  
  * .speed(f64): Sensitivity of the drag.  
  * .prefix(str) / .suffix(str): Adds units (e.g., "50 px").  
  * .fixed\_decimals(usize): formatting precision.17

### **5.5 Text Editing (TextEdit)**

A multiline or single-line text input field.

* **Constructors:** TextEdit::singleline(\&mut String) or TextEdit::multiline(\&mut String).  
* **Configuration:**  
  * .password(true): Masks characters with bullets.  
  * .hint\_text(str): Placeholder text when empty.  
  * .font(TextStyle): Overrides font (e.g., for code editors).  
  * .lock\_focus(true): Keeps focus even if the user clicks away (useful for console-like inputs).  
  * .desired\_width(f32): Sets the preferred width (use f32::INFINITY to fill space).  
  * .code\_editor(): Presets for monospaced font and tab handling.18

### **5.6 Images**

Renders a texture to a rectangle.

* **Constructor:** Image::new(texture\_id, size).  
* **Configuration:**  
  * .tint(Color32): Multiplies the texture color (useful for fading or coloring white icons).  
  * .rotate(f32): Rotates the image.  
  * .uv(Rect): Selects a sub-region (UV coordinates) of the texture, enabling spritesheets.19

## ---

**6\. Visuals, Styling, and Theming**

The aesthetic of an egui application is defined by the Style and Visuals structs. These can be modified globally (ctx.set\_visuals) or locally (ui.style\_mut).

### **6.1 The Visuals Struct**

This struct controls the color palette and stroke definitions for the UI.

| Field | Description | Usage Insight | Source |
| :---- | :---- | :---- | :---- |
| window\_fill | Background color of windows. | Main surface color. | 20 |
| panel\_fill | Background color of panels. | Often slightly distinct from windows. | 20 |
| extreme\_bg\_color | Dark/Deep background. | Used for input fields (TextEdit) to signal interactivity. | 20 |
| widgets | Widgets struct. | Contains WidgetVisuals for active, inactive, hovered states. | 20 |
| selection | Selection struct. | Colors for text highlighting and cursor. | 20 |
| window\_corner\_radius | Rounding of window corners. | Defines the "softness" of the UI. | 20 |
| window\_stroke | Border of windows. | Stroke { width, color }. | 20 |

Widget Visuals (WidgetVisuals):  
Nested within Visuals, this defines the exact look of interactive elements in different states:

* noninteractive: Labels, separators.  
* inactive: Buttons/inputs at rest.  
* hovered: Mouse-over state.  
* active: Mouse-down state.  
* open: Open menus/comboboxes.  
  Each state defines bg\_fill (background), fg\_stroke (text/icon color), bg\_stroke (border), and expansion (hover growth effect).20

### **6.2 Typography and Fonts**

Font management is handled via FontDefinitions.

* **Families:** FontFamily::Proportional (default UI font) and FontFamily::Monospace (code).  
* **Loading:** To add a font, you must load the binary data (TTF/OTF) into the definition context:  
  Rust  
  let mut fonts \= FontDefinitions::default();  
  fonts.font\_data.insert("my\_font".to\_owned(), FontData::from\_static(include\_bytes\!("my\_font.ttf")));  
  fonts.families.entry(FontFamily::Proportional).or\_default().insert(0, "my\_font".to\_owned());  
  ctx.set\_fonts(fonts);

* RichText: For individual styling, widgets accept RichText instead of strings:  
  ui.label(RichText::new("Alert").color(Color32::RED).size(20.0).strong()).21

## ---

**7\. Advanced Drawing: The Painter API**

When standard widgets are insufficient (e.g., node graphs, canvas editors, visualizations), the Painter API allows for direct, low-level drawing commands. Accessed via ui.painter(), it operates in the same coordinate space (logical points) as the widgets.

### **7.1 Primitive Shapes**

The Painter offers methods to draw primitives immediately. These are submitted to the GPU as tessellated triangles.

| Method | Parameters | Description | Source |
| :---- | :---- | :---- | :---- |
| rect\_filled | rect, radius, color | Draws a solid rounded rectangle. | 19 |
| rect\_stroke | rect, radius, stroke | Draws the outline of a rectangle. | 19 |
| circle\_filled | center, radius, color | Draws a solid circle. | 19 |
| line\_segment | \[p1, p2\], stroke | Draws a line between two points. | 19 |
| arrow | origin, vec, stroke | Draws a vector arrow. | 19 |
| image | tex\_id, rect, uv, tint | Renders a texture directly. | 19 |

### **7.2 Text Layout and Painting**

Drawing text manually is a two-step process involving layout (CPU expensive) and painting (GPU cheap).

1. **Layout:** The painter.layout(text, font\_id, color, width) method calculates line breaks and glyph positions, returning a Galley.  
2. **Painting:** The painter.galley(pos, galley) method renders the pre-calculated galley at the specified position.  
   * *Note:* painter.text(...) wraps this process into a single call for convenience but offers less control over reuse.19

### **7.3 Layers and Z-Ordering**

The Painter is associated with a specific LayerId. By default, it paints on the same layer as the Ui.

* **Ordering:** To paint *over* widgets (e.g., a drag-and-drop overlay), use ctx.layer\_painter(LayerId::new(Order::Tooltip, Id::new("overlay"))).  
* **Clipping:** The painter respects the clip\_rect of the Ui. To draw outside the container (e.g., a shadow extending beyond a window), you must use painter.with\_clip\_rect(...) with a larger bounds.19

## ---

**8\. State Management: The Memory System**

One of the greatest challenges in immediate mode is handling state that must persist while the UI is destroyed and recreated. Egui solves this with the Memory system.

### **8.1 The Id System**

Every widget in egui is identified by a 64-bit hash (Id). This Id is generated automatically based on the widget's label and its position in the hierarchy (the "Id path").

* **Collision Avoidance:** If two widgets have the same label (e.g., two "Edit" buttons in a list), their IDs will collide, causing them to share state (e.g., clicking one activates the other).  
* **Solution:** Use ui.push\_id(unique\_salt, |ui|...) to create a new ID namespace. This mixes the salt into the hash for all children.  
* **Explicit IDs:** ui.make\_persistent\_id(salt) allows creating a stable ID for storing data manually.7

### **8.2 Transient and Persistent Data**

The Memory struct provides storage for widget state.

* **data (IdTypeMap)**: Allows storing arbitrary Rust types associated with an Id.  
  * data.insert\_temp(id, value): Stored until the app restarts.  
  * data.insert\_persisted(id, value): Serialized to disk if persistence is enabled.  
  * *Usage:* This is how CollapsingHeader remembers if it is open, or how ScrollArea remembers offsets. Developers can use this for custom widgets (e.g., storing the pan/zoom level of a canvas).3

### **8.3 Computation Caches**

To avoid re-running expensive logic (like syntax highlighting or complex tesselation) every frame, Memory provides caches.

* **Mechanism:** ctx.memory\_mut(|m| m.caches.cache::\<MyCache\>().get(key, calculation\_closure)).  
* **Eviction:** The cache automatically evicts entries that haven't been accessed for a few frames, preventing memory leaks.3

## ---

**9\. Advanced Implementation Patterns**

### **9.1 Custom Widgets: The Widget Trait**

To create a reusable component that feels native to egui, implement the Widget trait.

* **Trait Definition:** fn ui(self, ui: \&mut Ui) \-\> Response.  
* **Pattern:**  
  1. **Allocate:** Use ui.allocate\_response(desired\_size, sense) to reserve space and get interaction data.  
  2. **Paint:** Use ui.painter() to draw the visual representation based on response.rect and response.hovered().  
  3. **Return:** Return the Response struct.  
* **Usage:** ui.add(MyCustomWidget::new(...)).22

### **9.2 Drag and Drop**

Egui supports drag and drop via the dnd (drag and drop) payload system on the Response object.

* **Source:** .dnd\_set\_drag\_payload(payload) marks a widget as draggable.  
* **Target:** ui.dnd\_drop\_zone::\<PayloadType, \_\>(...) detects if a payload is hovering over a region.  
* **Feedback:** Use ctx.is\_dragging\_payload() to conditionally render drop highlights.12

### **9.3 Accessibility**

Egui integrates with AccessKit for screen readers.

* **Semantics:** Use .widget\_info(|| WidgetInfo::new(...)) on a Response to describe custom widgets to assistive technology.  
* **Labelling:** response.labelled\_by(label\_id) links a label to an input for semantic association.12

## ---

**10\. Conclusion**

Egui represents a paradigm shift in Rust UI development. By treating the interface as an immediate reflection of state, it eliminates entire classes of synchronization bugs and simplifies the architecture of highly interactive applications. However, mastery of egui requires a deep understanding of its implicit mechanisms: the frame loop, space negotiation, ID hashing, and memory persistence. This reference provides the architectural blueprints and dense technical specifications necessary to leverage egui not just as a prototyping tool, but as a robust foundation for professional-grade desktop and web applications. The developer who masters the Context lock, the Painter primitives, and the Response interaction model wields complete control over the pixel-perfect rendering and behavioral logic of their application.