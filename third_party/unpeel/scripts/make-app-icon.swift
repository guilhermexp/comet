// Renders a macOS Big Sur-style app icon from flat square artwork:
// 1024x1024 transparent canvas, 824px squircle, glass rim, baked drop shadow.
// usage: swift make_unpeel_icon.swift <source.png> <out.png>

import AppKit
import CoreGraphics

let args = CommandLine.arguments
guard args.count >= 3 else {
    FileHandle.standardError.write("usage: make_unpeel_icon <src> <out>\n".data(using: .utf8)!)
    exit(1)
}
let srcURL = URL(fileURLWithPath: args[1])
let outURL = URL(fileURLWithPath: args[2])

guard let nsImage = NSImage(contentsOf: srcURL),
      let src = nsImage.cgImage(forProposedRect: nil, context: nil, hints: nil)
else {
    FileHandle.standardError.write("cannot load \(srcURL.path)\n".data(using: .utf8)!)
    exit(1)
}

let canvas = 1024
let space = CGColorSpace(name: CGColorSpace.sRGB)!
guard let ctx = CGContext(
    data: nil, width: canvas, height: canvas,
    bitsPerComponent: 8, bytesPerRow: 0, space: space,
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { exit(1) }

// Sample the source's background color from a corner pixel.
func sampleCorner(_ image: CGImage) -> CGColor {
    let w = image.width, h = image.height
    guard let sctx = CGContext(
        data: nil, width: w, height: h,
        bitsPerComponent: 8, bytesPerRow: w * 4, space: space,
        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
    ) else { return CGColor(red: 0.13, green: 0.12, blue: 0.13, alpha: 1) }
    sctx.draw(image, in: CGRect(x: 0, y: 0, width: w, height: h))
    guard let data = sctx.data else { return CGColor(red: 0.13, green: 0.12, blue: 0.13, alpha: 1) }
    let px = data.bindMemory(to: UInt8.self, capacity: w * h * 4)
    let off = (10 * w + 10) * 4
    return CGColor(
        red: CGFloat(px[off]) / 255, green: CGFloat(px[off + 1]) / 255,
        blue: CGFloat(px[off + 2]) / 255, alpha: 1
    )
}
let bg = sampleCorner(src)

// Apple icon grid: 824x824 squircle centered in 1024, corner radius ~185.
let iconRect = CGRect(x: 100, y: 100, width: 824, height: 824)
let radius: CGFloat = 185
let squircle = CGPath(roundedRect: iconRect, cornerWidth: radius, cornerHeight: radius, transform: nil)

// 1. Drop shadow under the squircle (dock-style, baked into the margins).
ctx.saveGState()
ctx.setShadow(
    offset: CGSize(width: 0, height: -10), blur: 26,
    color: CGColor(red: 0, green: 0, blue: 0, alpha: 0.38)
)
ctx.addPath(squircle)
ctx.setFillColor(bg)
ctx.fillPath()
ctx.restoreGState()

// 2. Artwork, aspect-filled inside the squircle.
ctx.saveGState()
ctx.addPath(squircle)
ctx.clip()
let scale = max(iconRect.width / CGFloat(src.width), iconRect.height / CGFloat(src.height))
let drawW = CGFloat(src.width) * scale
let drawH = CGFloat(src.height) * scale
ctx.interpolationQuality = .high
ctx.draw(src, in: CGRect(
    x: iconRect.midX - drawW / 2, y: iconRect.midY - drawH / 2,
    width: drawW, height: drawH
))

// 3. Glass sheen: faint white wash from the top, faint grounding at the bottom.
let sheen = CGGradient(
    colorsSpace: space,
    colors: [
        CGColor(red: 1, green: 1, blue: 1, alpha: 0.10),
        CGColor(red: 1, green: 1, blue: 1, alpha: 0.02),
        CGColor(red: 1, green: 1, blue: 1, alpha: 0.0),
    ] as CFArray,
    locations: [0.0, 0.35, 0.6]
)!
ctx.drawLinearGradient(
    sheen,
    start: CGPoint(x: 512, y: iconRect.maxY),
    end: CGPoint(x: 512, y: iconRect.minY),
    options: []
)
let ground = CGGradient(
    colorsSpace: space,
    colors: [
        CGColor(red: 0, green: 0, blue: 0, alpha: 0.14),
        CGColor(red: 0, green: 0, blue: 0, alpha: 0.0),
    ] as CFArray,
    locations: [0.0, 0.45]
)!
ctx.drawLinearGradient(
    ground,
    start: CGPoint(x: 512, y: iconRect.minY),
    end: CGPoint(x: 512, y: iconRect.maxY),
    options: []
)
ctx.restoreGState()

// 4. Glass rim: gradient stroke just inside the edge, brighter where lit.
func strokeRim(inset: CGFloat, width: CGFloat, topAlpha: CGFloat, bottomAlpha: CGFloat) {
    let rect = iconRect.insetBy(dx: inset, dy: inset)
    let path = CGPath(
        roundedRect: rect,
        cornerWidth: radius - inset, cornerHeight: radius - inset, transform: nil
    )
    let stroked = path.copy(
        strokingWithWidth: width, lineCap: .round, lineJoin: .round, miterLimit: 10
    )
    ctx.saveGState()
    ctx.addPath(squircle)
    ctx.clip()
    ctx.addPath(stroked)
    ctx.clip()
    let grad = CGGradient(
        colorsSpace: space,
        colors: [
            CGColor(red: 1, green: 1, blue: 1, alpha: topAlpha),
            CGColor(red: 1, green: 1, blue: 1, alpha: bottomAlpha),
        ] as CFArray,
        locations: [0.0, 1.0]
    )!
    ctx.drawLinearGradient(
        grad,
        start: CGPoint(x: 512, y: rect.maxY),
        end: CGPoint(x: 512, y: rect.minY),
        options: []
    )
    ctx.restoreGState()
}
strokeRim(inset: 1.75, width: 3.5, topAlpha: 0.55, bottomAlpha: 0.12)   // bright outer rim
strokeRim(inset: 5.5, width: 1.5, topAlpha: 0.18, bottomAlpha: 0.04)    // soft inner echo

guard let out = ctx.makeImage() else { exit(1) }
let rep = NSBitmapImageRep(cgImage: out)
rep.size = NSSize(width: 1024, height: 1024)
guard let png = rep.representation(using: .png, properties: [:]) else { exit(1) }
try! png.write(to: outURL)
print("wrote \(outURL.path)")
