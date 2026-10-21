// Draws the Forked app icon (1024×1024 PNG) with AppKit.
// Usage: swift scripts/make_icon.swift <output.png>
import AppKit

let size: CGFloat = 1024
let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon.png"

let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

// macOS icon grid: 824pt rounded square centered in 1024.
let inset: CGFloat = 100
let tile = NSRect(x: inset, y: inset, width: size - 2 * inset, height: size - 2 * inset)
let shape = NSBezierPath(roundedRect: tile, xRadius: 185, yRadius: 185)

let shadow = NSShadow()
shadow.shadowColor = NSColor.black.withAlphaComponent(0.35)
shadow.shadowOffset = NSSize(width: 0, height: -12)
shadow.shadowBlurRadius = 28
NSGraphicsContext.saveGraphicsState()
shadow.set()
NSColor.black.setFill()
shape.fill()
NSGraphicsContext.restoreGraphicsState()

let gradient = NSGradient(colors: [
    NSColor(red: 0.20, green: 0.45, blue: 0.98, alpha: 1),
    NSColor(red: 0.52, green: 0.30, blue: 0.92, alpha: 1),
])!
gradient.draw(in: shape, angle: -60)

// Branch glyph: a trunk with a branch forking off to the right.
let white = NSColor.white
let line: CGFloat = 54
let r: CGFloat = 70
let trunkX: CGFloat = 400
let bottom = NSPoint(x: trunkX, y: 300)
let top = NSPoint(x: trunkX, y: 724)
let side = NSPoint(x: 640, y: 600)

let path = NSBezierPath()
path.lineWidth = line
path.lineCapStyle = .round
path.move(to: bottom)
path.line(to: top)
path.move(to: NSPoint(x: trunkX, y: 380))
path.curve(
    to: NSPoint(x: side.x, y: side.y - r),
    controlPoint1: NSPoint(x: trunkX, y: 500),
    controlPoint2: NSPoint(x: side.x, y: 430))
white.setStroke()
path.stroke()

for p in [bottom, top, side] {
    let ring = NSBezierPath(ovalIn: NSRect(x: p.x - r, y: p.y - r, width: 2 * r, height: 2 * r))
    gradient.draw(in: ring, angle: -60)
    ring.lineWidth = line * 0.85
    white.setStroke()
    ring.stroke()
}

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
