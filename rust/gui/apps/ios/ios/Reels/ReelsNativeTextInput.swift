import UIKit

private final class ReelsNativeTextView: UITextView {
    let fieldID: String
    var onPaste: ((ReelsNativeTextView) -> Void)?

    init(fieldID: String) {
        self.fieldID = fieldID
        super.init(frame: .zero, textContainer: nil)
        backgroundColor = .clear
        isOpaque = false
        isScrollEnabled = true
        isEditable = true
        isSelectable = true
        isUserInteractionEnabled = true
        clipsToBounds = true
        alwaysBounceVertical = false
        keyboardDismissMode = .interactive
        dataDetectorTypes = []
        autocorrectionType = .default
        autocapitalizationType = .none
        spellCheckingType = .default
        smartQuotesType = .no
        smartDashesType = .no
        textContainer.lineFragmentPadding = 0
        textContainerInset = .zero
        contentInset = .zero
        scrollIndicatorInsets = .zero
        textDragInteraction?.isEnabled = true
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc override func paste(_ sender: Any?) {
        super.paste(sender)
        onPaste?(self)
    }
}

/// The border and placeholder live outside the UIScrollView that owns the
/// text. A UIScrollView moves its bounds origin while scrolling, so a border
/// layer attached to UITextView would travel with the text. This container
/// never scrolls; only its transparent UITextView subview does.
private final class ReelsNativeInputContainer: UIView {
    let fieldID: String
    let placeholderLabel = UILabel()
    let borderLayer = CAShapeLayer()
    var normalBorderColor = UIColor.clear
    var focusBorderColor = UIColor.blue
    var borderWidth: CGFloat = 1
    var borderCornerRadius: CGFloat = 6
    var placeholderTopConstraint: NSLayoutConstraint?
    var placeholderLeadingConstraint: NSLayoutConstraint?
    var placeholderTrailingConstraint: NSLayoutConstraint?

    init(fieldID: String) {
        self.fieldID = fieldID
        super.init(frame: .zero)
        backgroundColor = .clear
        isOpaque = false
        clipsToBounds = true
        isHidden = true
        isUserInteractionEnabled = true

        borderLayer.fillColor = UIColor.clear.cgColor
        borderLayer.lineWidth = 1
        borderLayer.zPosition = 100
        layer.addSublayer(borderLayer)

        placeholderLabel.numberOfLines = 0
        placeholderLabel.lineBreakMode = .byCharWrapping
        placeholderLabel.translatesAutoresizingMaskIntoConstraints = false
        placeholderLabel.isUserInteractionEnabled = false
        placeholderLabel.layer.zPosition = 101
        addSubview(placeholderLabel)
        let top = placeholderLabel.topAnchor.constraint(equalTo: topAnchor)
        let leading = placeholderLabel.leadingAnchor.constraint(equalTo: leadingAnchor)
        // Equal (not lessThanOrEqual) so the label always takes the field's
        // full text width; an open-ended trailing bound let Auto Layout pick
        // a narrower width and wrap the placeholder early.
        let trailing = placeholderLabel.trailingAnchor.constraint(equalTo: trailingAnchor)
        placeholderTopConstraint = top
        placeholderLeadingConstraint = leading
        placeholderTrailingConstraint = trailing
        NSLayoutConstraint.activate([top, leading, trailing])
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        updateBorderShape()
    }

    func applyBorder(normal: UIColor, focus: UIColor, width: CGFloat, radius: CGFloat) {
        normalBorderColor = normal
        focusBorderColor = focus
        borderWidth = max(width, 0.5)
        borderCornerRadius = radius
        borderLayer.lineWidth = borderWidth
        updateBorderShape()
    }

    func setFocusedAppearance(_ focused: Bool) {
        borderLayer.strokeColor = (focused ? focusBorderColor : normalBorderColor).cgColor
        updateBorderShape()
    }

    func updateBorderShape() {
        borderLayer.frame = bounds
        let inset = borderWidth / 2
        let rect = CGRect(
            x: inset,
            y: inset,
            width: max(bounds.width - inset * 2, 0),
            height: max(bounds.height - inset * 2, 0)
        )
        borderLayer.path = UIBezierPath(
            roundedRect: rect,
            cornerRadius: max(borderCornerRadius - inset, 0)
        ).cgPath
    }
}

final class ReelsNativeTextInputManager: NSObject, UITextViewDelegate {
    static let shared = ReelsNativeTextInputManager()

    private var editors: [String: ReelsNativeTextView] = [:]
    private var containers: [String: ReelsNativeInputContainer] = [:]
    private var activeID: String?
    private var placeholderIDs: Set<String> = []
    private var applyingProgrammaticText = false
    private var frameRequestPending = false
    private var hideGenerations: [String: Int] = [:]
    private var showGenerations: [String: Int] = [:]
    private var suppressTextChanges: Set<String> = []
    private var clipRects: [String: CGRect] = [:]
    private var pasteInFlight: Set<String> = []

    private func hostView() -> UIView? {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let window = scenes
            .flatMap(\.windows)
            .first(where: \.isKeyWindow)
            ?? scenes.flatMap(\.windows).first
        let host = window?.rootViewController?.view
        return host
    }

    private func makeEditorIfNeeded(
        for fieldID: String
    ) -> (ReelsNativeTextView, ReelsNativeInputContainer)? {
        if let editor = editors[fieldID], let container = containers[fieldID] {
            return (editor, container)
        }
        guard let host = hostView() else {
            return nil
        }
        let container = ReelsNativeInputContainer(fieldID: fieldID)
        let editor = ReelsNativeTextView(fieldID: fieldID)
        editor.delegate = self
        editor.onPaste = { [weak self] pastedEditor in
            self?.reportTextChange(pastedEditor)
        }
        editor.frame = container.bounds
        editor.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        container.addSubview(editor)
        host.addSubview(container)
        containers[fieldID] = container
        editors[fieldID] = editor
        return (editor, container)
    }

    private func editor(for fieldID: String) -> ReelsNativeTextView? {
        makeEditorIfNeeded(for: fieldID)?.0
    }

    private func container(for fieldID: String) -> ReelsNativeInputContainer? {
        makeEditorIfNeeded(for: fieldID)?.1
    }

    func paste(fieldID: String) {
        guard let editor = editors[fieldID], let container = containers[fieldID],
              !container.isHidden else {
            return
        }
        guard !pasteInFlight.contains(fieldID) else {
            return
        }
        pasteInFlight.insert(fieldID)
        allowTextChanges(fieldID: fieldID)
        bringContainerToFront(container)
        if !editor.isFirstResponder {
            _ = editor.becomeFirstResponder()
        }
        // The Paste action button replaces the current value, matching the
        // desktop/Android buttons. UIKit's default insert-at-caret would
        // append another copy of the clipboard text on every tap.
        let currentText = editor.text ?? ""
        editor.selectedRange = NSRange(
            location: 0,
            length: (currentText as NSString).length
        )
        editor.paste(nil)
        // iOS can ask for paste permission before it inserts the text. The
        // delegate callback fires while the editor is still empty; report the
        // settled text after the system sheet closes so Rust can grow its
        // transparent slot and enable the action buttons in the same frame.
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) { [weak self, weak editor] in
            guard let self, let editor else {
                return
            }
            self.pasteInFlight.remove(fieldID)
            self.allowTextChanges(fieldID: fieldID)
            self.reportTextChange(editor)
        }
    }

    func show(
        fieldID: String,
        text: String,
        selectionStart: Int,
        selectionLength: Int,
        frame: CGRect,
        clipRect: CGRect?,
        textOrigin: CGPoint,
        fontSize: CGFloat,
        multiline: Bool,
        wrapWidth: CGFloat,
        lineHeight: CGFloat,
        focus: Bool,
        focused: Bool,
        forceTextSync: Bool,
        textColor: UIColor,
        backgroundColor: UIColor,
        borderColor: UIColor,
        focusBorderColor: UIColor,
        borderWidth: CGFloat,
        placeholder: String,
        placeholderColor: UIColor,
        tintColor: UIColor,
        cornerRadius: CGFloat,
        minHeight: CGFloat,
        maxHeight: CGFloat
    ) {
        guard let editor = editor(for: fieldID), let container = container(for: fieldID) else {
            return
        }
        _ = (backgroundColor, minHeight, maxHeight)

        placeholderIDs.insert(fieldID)
        let generation = (showGenerations[fieldID] ?? 0) + 1
        showGenerations[fieldID] = generation
        hideGenerations[fieldID] = generation
        allowTextChanges(fieldID: fieldID)
        if focus {
            activeID = fieldID
        }
        // The placeholder's prepaint owns this field's visible state. Any
        // earlier async resign/hide request must not win over this frame.
        hideExcept(fieldID: fieldID, generation: generation)

        let bounds = CGRect(
            x: frame.minX,
            y: frame.minY,
            width: max(frame.width, 1),
            height: max(frame.height, 1)
        )
        container.frame = bounds
        editor.isHidden = false
        editor.isUserInteractionEnabled = true
        editor.frame = container.bounds
        container.isHidden = false
        container.isUserInteractionEnabled = true
        editor.backgroundColor = .clear
        editor.textColor = textColor
        editor.tintColor = tintColor
        editor.font = UIFont.systemFont(ofSize: fontSize)

        let textInsetLeft = max(textOrigin.x - frame.minX, 0)
        let textInsetTop = max(textOrigin.y - frame.minY, 0)
        let textInsetRight = max(bounds.width - textInsetLeft - wrapWidth, 0)
        editor.textContainerInset = UIEdgeInsets(
            top: textInsetTop,
            left: textInsetLeft,
            bottom: textInsetTop,
            right: textInsetRight
        )

        container.applyBorder(
            normal: borderColor,
            focus: focusBorderColor,
            width: borderWidth,
            radius: cornerRadius
        )
        container.setFocusedAppearance(focused || focus || editor.isFirstResponder)
        clipRects[fieldID] = clipRect
        applyClip(clipRect, to: container)
        editor.showsVerticalScrollIndicator = multiline
        editor.indicatorStyle = textColor.isDark ? .black : .white
        container.placeholderLabel.text = placeholder
        container.placeholderLabel.textColor = placeholderColor
        container.placeholderLabel.font = editor.font
        container.placeholderLabel.textAlignment = .natural
        container.placeholderLabel.lineBreakMode = .byCharWrapping
        container.placeholderTopConstraint?.constant = textInsetTop
        container.placeholderLeadingConstraint?.constant = textInsetLeft
        container.placeholderTrailingConstraint?.constant = -textInsetRight
        container.placeholderLabel.isHidden = !(editor.text ?? "").isEmpty

        let paragraph = NSMutableParagraphStyle()
        if lineHeight > 0 {
            paragraph.minimumLineHeight = lineHeight
            paragraph.maximumLineHeight = lineHeight
            paragraph.lineSpacing = 0
        }
        editor.typingAttributes = [
            .font: editor.font as Any,
            .foregroundColor: textColor,
            .paragraphStyle: paragraph,
        ]

        if forceTextSync || (editor.text ?? "") != text {
            applyProgrammaticText(
                text,
                to: editor,
                selectionStart: selectionStart,
                selectionLength: selectionLength
            )
        }
        container.placeholderLabel.isHidden = !(editor.text ?? "").isEmpty

        bringContainerToFront(container)
        if focus {
            becomeFirstResponderWhenReady(
                editor,
                container: container,
                fieldID: fieldID,
                generation: generation
            )
        }
    }

    func hide(fieldID: String?) {
        finishEditing(fieldID: fieldID, keepVisible: false)
    }

    func endEditing(fieldID: String?) {
        finishEditing(fieldID: fieldID, keepVisible: true)
    }

    func focus(fieldID: String) {
        guard let editor = editors[fieldID], let container = containers[fieldID],
              !container.isHidden else {
            return
        }
        let generation = (showGenerations[fieldID] ?? 0) + 1
        showGenerations[fieldID] = generation
        hideGenerations[fieldID] = generation
        activeID = fieldID
        editor.isUserInteractionEnabled = true
        container.isUserInteractionEnabled = true
        container.setFocusedAppearance(true)
        bringContainerToFront(container)
        becomeFirstResponderWhenReady(
            editor,
            container: container,
            fieldID: fieldID,
            generation: generation
        )
    }

    func hideExcept(fieldID: String, generation: Int? = nil) {
        for (id, container) in containers where id != fieldID {
            if activeID == id {
                activeID = nil
            }
            placeholderIDs.remove(id)
            hideGenerations[id] = (hideGenerations[id] ?? 0) + 1
            editors[id]?.resignFirstResponder()
            container.isHidden = true
            container.isUserInteractionEnabled = false
        }
        // A pending hide request for the field being shown belongs to an
        // earlier generation. Its async completion must not hide this frame.
        if let generation {
            hideGenerations[fieldID] = generation
        }
    }

    func cancelComposing(fieldID: String) {
        guard let editor = editors[fieldID] else {
            return
        }
        suppressTextChanges.insert(fieldID)
        editor.resignFirstResponder()
    }

    func allowTextChanges(fieldID: String) {
        suppressTextChanges.remove(fieldID)
    }

    func replaceText(fieldID: String, text: String) {
        guard let editor = editors[fieldID] else {
            return
        }
        defer { suppressTextChanges.remove(fieldID) }
        if editor.isFirstResponder {
            editor.resignFirstResponder()
        }
        applyProgrammaticText(text, to: editor, selectionStart: 0, selectionLength: 0)
        containers[fieldID]?.placeholderLabel.isHidden = !text.isEmpty
        editor.contentOffset = .zero
    }

    func move(fieldID: String, x: CGFloat, y: CGFloat) {
        guard let container = containers[fieldID], !container.isHidden else {
            return
        }
        var frame = container.frame
        frame.origin.x = x
        frame.origin.y = y
        container.frame = frame
        if let editor = editors[fieldID] {
            editor.frame = CGRect(origin: .zero, size: container.bounds.size)
        }
        container.updateBorderShape()
        applyClip(clipRects[fieldID], to: container)
    }

    private func finishEditing(fieldID: String?, keepVisible: Bool) {
        let ids: [String]
        if let fieldID {
            ids = [fieldID]
        } else {
            ids = Array(editors.keys)
        }
        for id in ids {
            guard let editor = editors[id], let container = containers[id] else {
                continue
            }
            if activeID == id {
                activeID = nil
            }
            if !keepVisible {
                placeholderIDs.remove(id)
            }
            let generation = (hideGenerations[id] ?? 0) + 1
            hideGenerations[id] = generation
            finishEditingWhenReady(
                editor,
                container: container,
                keepVisible: keepVisible,
                generation: generation
            )
        }
    }

    private func finishEditingWhenReady(
        _ editor: ReelsNativeTextView,
        container: ReelsNativeInputContainer,
        keepVisible: Bool,
        generation: Int
    ) {
        DispatchQueue.main.async { [weak self, weak editor] in
            guard let editor else {
                return
            }
            guard let self, self.hideGenerations[editor.fieldID] == generation else {
                return
            }
            editor.resignFirstResponder()
            let isPlaceholder = self.placeholderIDs.contains(editor.fieldID)
            let hidden = !keepVisible || !isPlaceholder
            editor.isHidden = hidden
            editor.isUserInteractionEnabled = isPlaceholder
            container.isHidden = hidden
            container.isUserInteractionEnabled = isPlaceholder
            container.setFocusedAppearance(false)
            self.notifyGPUI()
        }
    }

    private func applyProgrammaticText(
        _ text: String,
        to editor: ReelsNativeTextView,
        selectionStart: Int,
        selectionLength: Int
    ) {
        applyingProgrammaticText = true
        defer { applyingProgrammaticText = false }
        let selection = editor.isFirstResponder ? editor.selectedRange : nil
        let attributes: [NSAttributedString.Key: Any] = editor.typingAttributes
        editor.attributedText = NSAttributedString(string: text, attributes: attributes)
        let utf16Length = (text as NSString).length
        let start: Int
        let length: Int
        if let selection, selection.location <= utf16Length, NSMaxRange(selection) <= utf16Length {
            start = selection.location
            length = selection.length
        } else {
            start = min(max(selectionStart, 0), utf16Length)
            let end = min(start + max(selectionLength, 0), utf16Length)
            length = end - start
        }
        editor.selectedRange = NSRange(location: start, length: length)
        containers[editor.fieldID]?.placeholderLabel.isHidden = !text.isEmpty
        editor.contentOffset = .zero
        editor.layoutIfNeeded()
    }

    private func becomeFirstResponderWhenReady(
        _ editor: ReelsNativeTextView,
        container: ReelsNativeInputContainer,
        fieldID: String,
        generation: Int,
        attempt: Int = 0
    ) {
        DispatchQueue.main.async { [weak self, weak editor] in
            guard let self, let editor,
                  self.activeID == fieldID,
                  self.showGenerations[fieldID] == generation,
                  !editor.isHidden, !container.isHidden else {
                return
            }
            editor.isUserInteractionEnabled = true
            container.isUserInteractionEnabled = true
            self.bringContainerToFront(container)
            if !editor.isFirstResponder {
                _ = editor.becomeFirstResponder()
            }
            if !editor.isFirstResponder, attempt < 2 {
                DispatchQueue.main.asyncAfter(deadline: .now() + (attempt == 0 ? 0.08 : 0.2)) {
                    self.becomeFirstResponderWhenReady(
                        editor,
                        container: container,
                        fieldID: fieldID,
                        generation: generation,
                        attempt: attempt + 1
                    )
                }
            }
        }
    }

    private func bringContainerToFront(_ container: UIView) {
        container.superview?.bringSubviewToFront(container)
    }

    private func applyClip(_ clipRect: CGRect?, to container: UIView) {
        guard let clipRect else {
            container.layer.mask = nil
            return
        }
        let localClip = CGRect(
            x: clipRect.minX - container.frame.minX,
            y: clipRect.minY - container.frame.minY,
            width: clipRect.width,
            height: clipRect.height
        ).intersection(container.bounds)
        let mask = (container.layer.mask as? CAShapeLayer) ?? CAShapeLayer()
        mask.frame = container.bounds
        mask.path = UIBezierPath(rect: localClip).cgPath
        container.layer.mask = mask
    }

    private func notifyGPUI() {
        guard !frameRequestPending else {
            return
        }
        frameRequestPending = true
        DispatchQueue.main.async { [weak self] in
            guard let self else {
                return
            }
            self.frameRequestPending = false
            if let window = gpui_ios_get_window() {
                gpui_ios_request_frame(window)
            }
        }
    }

    func textViewDidBeginEditing(_ textView: UITextView) {
        guard let editor = textView as? ReelsNativeTextView else {
            return
        }
        activeID = editor.fieldID
        editor.isUserInteractionEnabled = true
        containers[editor.fieldID]?.setFocusedAppearance(true)
        editor.fieldID.withCString {
            reels_native_text_input_focus_changed($0, 1)
        }
        notifyGPUI()
    }

    func textViewDidEndEditing(_ textView: UITextView) {
        guard let editor = textView as? ReelsNativeTextView else {
            return
        }
        if activeID == editor.fieldID {
            activeID = nil
        }
        containers[editor.fieldID]?.setFocusedAppearance(false)
        editor.fieldID.withCString {
            reels_native_text_input_focus_changed($0, 0)
        }
        notifyGPUI()
    }

    func textViewDidChange(_ textView: UITextView) {
        guard !applyingProgrammaticText,
              !suppressTextChanges.contains((textView as? ReelsNativeTextView)?.fieldID ?? ""),
              let editor = textView as? ReelsNativeTextView else {
            return
        }
        reportTextChange(editor)
    }

    private func reportTextChange(_ editor: ReelsNativeTextView) {
        guard !applyingProgrammaticText,
              !suppressTextChanges.contains(editor.fieldID) else {
            return
        }
        let text = editor.text ?? ""
        containers[editor.fieldID]?.placeholderLabel.isHidden = !text.isEmpty
        let selection = editor.selectedRange
        editor.fieldID.withCString { idPointer in
            text.withCString { textPointer in
                reels_native_text_input_changed(
                    idPointer,
                    textPointer,
                    selection.location,
                    selection.length
                )
            }
        }
        notifyMeasuredHeight(editor, text: text)
        notifyGPUI()
    }

    /// Swift owns the visible text, so its text-container measurement is the
    /// authoritative number of wrapped rows. GPUI reserves a transparent slot
    /// and uses this height to keep the native border and action buttons from
    /// being overdrawn by long pasted text.
    private func notifyMeasuredHeight(_ editor: ReelsNativeTextView, text: String) {
        editor.layoutIfNeeded()
        let contentHeight = editor.sizeThatFits(
            CGSize(width: editor.bounds.width, height: .greatestFiniteMagnitude)
        ).height
        // Rust converts the reported height back to rows with
        // `(height - 24) / lineHeight`, where 24 pt is the slot chrome Rust
        // reserves around the text (top padding + bottom padding).
        // sizeThatFits includes the real top text-container inset, so strip
        // both inset edges before adding the chrome constant; otherwise the reported height
        // is smaller than the rendered content and the transparent slot
        // under-measures the editor.
        let chrome = 24.0
        let measured =
            contentHeight - editor.textContainerInset.top - editor.textContainerInset.bottom + chrome
        editor.fieldID.withCString { idPointer in
            text.withCString { textPointer in
                reels_native_text_input_height_changed(
                    idPointer,
                    textPointer,
                    Float(measured)
                )
            }
        }
    }

    func textViewDidChangeSelection(_ textView: UITextView) {
        guard !applyingProgrammaticText,
              let editor = textView as? ReelsNativeTextView else {
            return
        }
        let selection = editor.selectedRange
        editor.fieldID.withCString {
            reels_native_text_input_selection_changed(
                $0,
                selection.location,
                selection.length
            )
        }
    }
}

private extension UIColor {
    var isDark: Bool {
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        var alpha: CGFloat = 0
        guard getRed(&red, green: &green, blue: &blue, alpha: &alpha) else {
            return true
        }
        return (0.299 * red + 0.587 * green + 0.114 * blue) < 0.5
    }
}

private func colorFromRGBA(_ value: UInt32) -> UIColor {
    UIColor(
        red: CGFloat((value >> 24) & 0xff) / 255,
        green: CGFloat((value >> 16) & 0xff) / 255,
        blue: CGFloat((value >> 8) & 0xff) / 255,
        alpha: CGFloat(value & 0xff) / 255
    )
}

@_cdecl("reels_ios_native_input_show")
public func reelsIOSNativeInputShow(
    _ idPointer: UnsafePointer<CChar>?,
    _ textPointer: UnsafePointer<CChar>?,
    _ selectionStart: Int,
    _ selectionLength: Int,
    _ x: Float,
    _ y: Float,
    _ width: Float,
    _ height: Float,
    _ clipX: Float,
    _ clipY: Float,
    _ clipWidth: Float,
    _ clipHeight: Float,
    _ textX: Float,
    _ textY: Float,
    _ fontSize: Float,
    _ multiline: Int32,
    _ wrapWidth: Float,
    _ lineHeight: Float,
    _ focus: Int32,
    _ focused: Int32,
    _ forceTextSync: Int32,
    _ textColor: UInt32,
    _ backgroundColor: UInt32,
    _ borderColor: UInt32,
    _ focusBorderColor: UInt32,
    _ borderWidth: Float,
    _ placeholderPointer: UnsafePointer<CChar>?,
    _ placeholderColor: UInt32,
    _ tintColor: UInt32,
    _ cornerRadius: Float,
    _ minHeight: Float,
    _ maxHeight: Float
) {
    guard let idPointer, let textPointer, let placeholderPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.show(
        fieldID: String(cString: idPointer),
        text: String(cString: textPointer),
        selectionStart: selectionStart,
        selectionLength: selectionLength,
        frame: CGRect(
            x: CGFloat(x),
            y: CGFloat(y),
            width: CGFloat(width),
            height: CGFloat(height)
        ),
        clipRect: clipWidth >= 0 && clipHeight >= 0
            ? CGRect(
                x: CGFloat(clipX),
                y: CGFloat(clipY),
                width: CGFloat(clipWidth),
                height: CGFloat(clipHeight)
            )
            : nil,
        textOrigin: CGPoint(x: CGFloat(textX), y: CGFloat(textY)),
        fontSize: CGFloat(fontSize),
        multiline: multiline != 0,
        wrapWidth: CGFloat(wrapWidth),
        lineHeight: CGFloat(lineHeight),
        focus: focus != 0,
        focused: focused != 0,
        forceTextSync: forceTextSync != 0,
        textColor: colorFromRGBA(textColor),
        backgroundColor: colorFromRGBA(backgroundColor),
        borderColor: colorFromRGBA(borderColor),
        focusBorderColor: colorFromRGBA(focusBorderColor),
        borderWidth: CGFloat(borderWidth),
        placeholder: String(cString: placeholderPointer),
        placeholderColor: colorFromRGBA(placeholderColor),
        tintColor: colorFromRGBA(tintColor),
        cornerRadius: CGFloat(cornerRadius),
        minHeight: CGFloat(minHeight),
        maxHeight: CGFloat(maxHeight)
    )
}

@_cdecl("reels_ios_native_input_hide")
public func reelsIOSNativeInputHide(_ idPointer: UnsafePointer<CChar>?) {
    guard let idPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.hide(fieldID: String(cString: idPointer))
}

@_cdecl("reels_ios_native_input_move")
public func reelsIOSNativeInputMove(
    _ idPointer: UnsafePointer<CChar>?,
    _ x: Float,
    _ y: Float
) {
    guard let idPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.move(
        fieldID: String(cString: idPointer),
        x: CGFloat(x),
        y: CGFloat(y)
    )
}

@_cdecl("reels_ios_native_input_hide_except")
public func reelsIOSNativeInputHideExcept(_ idPointer: UnsafePointer<CChar>?) {
    guard let idPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.hideExcept(fieldID: String(cString: idPointer))
}

@_cdecl("reels_ios_native_input_end_editing")
public func reelsIOSNativeInputEndEditing(_ idPointer: UnsafePointer<CChar>?) {
    guard let idPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.endEditing(fieldID: String(cString: idPointer))
}

@_cdecl("reels_ios_native_input_focus")
public func reelsIOSNativeInputFocus(_ idPointer: UnsafePointer<CChar>?) {
    guard let idPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.focus(fieldID: String(cString: idPointer))
}

@_cdecl("reels_ios_native_input_paste")
public func reelsIOSNativeInputPaste(_ idPointer: UnsafePointer<CChar>?) {
    guard let idPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.paste(fieldID: String(cString: idPointer))
}

@_cdecl("reels_ios_native_input_cancel_composing")
public func reelsIOSNativeInputCancelComposing(_ idPointer: UnsafePointer<CChar>?) {
    guard let idPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.cancelComposing(fieldID: String(cString: idPointer))
}

@_cdecl("reels_ios_native_input_replace_text")
public func reelsIOSNativeInputReplaceText(
    _ idPointer: UnsafePointer<CChar>?,
    _ textPointer: UnsafePointer<CChar>?
) {
    guard let idPointer, let textPointer else {
        return
    }
    ReelsNativeTextInputManager.shared.replaceText(
        fieldID: String(cString: idPointer),
        text: String(cString: textPointer)
    )
}
