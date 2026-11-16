import AppKit
import CoreText
import Foundation

struct Options {
    var font: NSFont
    var size: CGFloat
    var text: String
    var padding: CGFloat
    var perCharacter: Bool
    var gradientAngleDegrees: CGFloat
}

func glyphPaths(_ options: Options) -> [(CGPath, CGRect)] {
    let attributes: [NSAttributedString.Key: Any] = [.font: options.font]
    let attributed = NSAttributedString(string: options.text, attributes: attributes)
    let line = CTLineCreateWithAttributedString(attributed)

    let runs = CTLineGetGlyphRuns(line) as! [CTRun]
    var result: [(CGPath, CGRect)] = []
    for run in runs {
        let count = CTRunGetGlyphCount(run)
        var glyphs = [CGGlyph](repeating: 0, count: count)
        var positions = [CGPoint](repeating: .zero, count: count)
        CTRunGetGlyphs(run, CFRange(location: 0, length: count), &glyphs)
        CTRunGetPositions(run, CFRange(location: 0, length: count), &positions)

        for index in 0..<count {
            guard let glyphPath = CTFontCreatePathForGlyph(options.font, glyphs[index], nil) else {
                continue
            }
            var transform = CGAffineTransform(translationX: positions[index].x, y: positions[index].y)
            let transformedPath = glyphPath.copy(using: &transform) ?? glyphPath
            result.append((transformedPath, transformedPath.boundingBox))
        }
    }

    return result
}

func svgPath(_ path: CGPath) -> String {
    var commands: [String] = []
    func value(_ value: CGFloat) -> String {
        String(format: "%.2f", value)
    }
    path.applyWithBlock { elementPointer in
        let element = elementPointer.pointee
        let points = UnsafeBufferPointer(start: element.points, count: 3)
        switch element.type {
        case .moveToPoint:
            commands.append("M\(value(points[0].x)) \(value(points[0].y))")
        case .addLineToPoint:
            commands.append("L\(value(points[0].x)) \(value(points[0].y))")
        case .addQuadCurveToPoint:
            commands.append("Q\(value(points[0].x)) \(value(points[0].y)) \(value(points[1].x)) \(value(points[1].y))")
        case .addCurveToPoint:
            commands.append("C\(value(points[0].x)) \(value(points[0].y)) \(value(points[1].x)) \(value(points[1].y)) \(value(points[2].x)) \(value(points[2].y))")
        case .closeSubpath:
            commands.append("Z")
        @unknown default:
            break
        }
    }
    return commands.joined(separator: " ")
}

func escape(_ value: String) -> String {
    value.replacingOccurrences(of: "-", with: "_")
}

func round(_ value: CGFloat) -> String {
    String(format: "%.2f", value)
}

func makeSVG(_ options: Options) -> String {
    let glyphs = glyphPaths(options)
    var minX = CGFloat.greatestFiniteMagnitude
    var minY = CGFloat.greatestFiniteMagnitude
    var maxX = -CGFloat.greatestFiniteMagnitude
    var maxY = -CGFloat.greatestFiniteMagnitude
    for (_, bounds) in glyphs {
        minX = min(minX, bounds.minX)
        minY = min(minY, bounds.minY)
        maxX = max(maxX, bounds.maxX)
        maxY = max(maxY, bounds.maxY)
    }
    let width = max(maxX - minX, 1)
    let height = max(maxY - minY, 1)
    // SVG 路径空间 y 轴向上，屏幕顺时针角度对应 dy 取负。
    let angle = options.gradientAngleDegrees * .pi / 180
    let directionX = cos(angle)
    let directionY = -sin(angle)
    var defs: [String] = []
    var paths: [String] = []
    for (index, (path, bounds)) in glyphs.enumerated() {
        let spanMinX = options.perCharacter ? bounds.minX : minX
        let spanMaxX = options.perCharacter ? bounds.maxX : maxX
        let spanMinY = options.perCharacter ? bounds.minY : minY
        let spanMaxY = options.perCharacter ? bounds.maxY : maxY
        let projections = [
            spanMinX * directionX + spanMinY * directionY,
            spanMaxX * directionX + spanMinY * directionY,
            spanMinX * directionX + spanMaxY * directionY,
            spanMaxX * directionX + spanMaxY * directionY,
        ]
        let projectionStart = projections.min() ?? 0
        let projectionEnd = projections.max() ?? width
        let gradientStartX = projectionStart * directionX
        let gradientStartY = projectionStart * directionY
        let gradientEndX = projectionEnd * directionX
        let gradientEndY = projectionEnd * directionY
        let gradientID = "reels-gradient-\(index)"
        defs.append("""
          <linearGradient id="\(gradientID)" x1="\(round(gradientStartX))" y1="\(round(gradientStartY))" x2="\(round(gradientEndX))" y2="\(round(gradientEndY))" gradientUnits="userSpaceOnUse">
            <stop offset="0" stop-color="#0A78F8"/>
            <stop offset="0.35" stop-color="#3E77F7"/>
            <stop offset="0.65" stop-color="#7C56EC"/>
            <stop offset="1" stop-color="#A34BE0"/>
          </linearGradient>
        """)
        let transform = "translate(\(round(-minX)),\(round(maxY))) scale(1,-1)"
        paths.append("""
          <path d="\(svgPath(path))" transform="\(transform)" fill="url(#\(gradientID))"/>
        """)
    }
    return """
    <svg xmlns="http://www.w3.org/2000/svg" width="\(round(width))" height="\(round(height))" viewBox="0 0 \(round(width)) \(round(height))" fill="none">
      <defs>
      \(defs.joined(separator: "\n"))
      </defs>
      \(paths.joined(separator: "\n"))
    </svg>
    """
}

func output(name: String, text: String, size: CGFloat, perCharacter: Bool = false, gradientAngleDegrees: CGFloat = 0) {
    guard let font = NSFont(name: "PingFang SC", size: size) else {
        fatalError("Missing PingFang SC")
    }
    let svg = makeSVG(Options(font: font, size: size, text: text, padding: 0, perCharacter: perCharacter, gradientAngleDegrees: gradientAngleDegrees))
    let fileURL = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
        .appendingPathComponent("rust/assets/gradient/\(name).svg")
    try! svg.data(using: .utf8)!.write(to: fileURL)
    print("\(name): \(svg.count) bytes")
}

output(name: "title", text: "短视频解析", size: 60)
output(name: "paste", text: "粘贴", size: 42, perCharacter: true, gradientAngleDegrees: 30)
output(name: "clear", text: "清空", size: 42, perCharacter: true, gradientAngleDegrees: 30)
output(name: "parse", text: "解析", size: 42, perCharacter: true, gradientAngleDegrees: 30)
output(name: "export-script", text: "导出下载脚本", size: 42, perCharacter: true, gradientAngleDegrees: 30)
