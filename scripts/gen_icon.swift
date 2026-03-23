#!/usr/bin/swift
// PetruTerm — app icon generator
// Produces: assets/AppIcon.png  (1024×1024, sRGB)
// Run: swift scripts/gen_icon.swift
//
// Design:
//   • macOS Big-Sur-style rounded rectangle
//   • Dracula Pro dark background with subtle radial glow
//   • Purple ">" chevron (Dracula #9580ff) + cursor block
//   • Thin horizontal scan-line accent at the bottom third

import Foundation
import CoreGraphics
import ImageIO

let px = 1024
let W  = CGFloat(px)
let H  = CGFloat(px)

// ── canvas ────────────────────────────────────────────────────────────────────
guard let ctx = CGContext(
    data: nil,
    width:  px,
    height: px,
    bitsPerComponent: 8,
    bytesPerRow: px * 4,
    space: CGColorSpaceCreateDeviceRGB(),
    bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
) else { fatalError("CGContext failed") }

// CoreGraphics is bottom-up; flip so (0,0) is top-left
ctx.translateBy(x: 0, y: H)
ctx.scaleBy(x: 1, y: -1)

// ── helpers ───────────────────────────────────────────────────────────────────
func srgb(_ r: CGFloat, _ g: CGFloat, _ b: CGFloat, _ a: CGFloat = 1) -> CGColor {
    CGColor(colorSpace: CGColorSpaceCreateDeviceRGB(),
            components: [r, g, b, a])!
}
func hex(_ h: UInt32, _ a: CGFloat = 1) -> CGColor {
    srgb(CGFloat((h >> 16) & 0xff) / 255,
         CGFloat((h >>  8) & 0xff) / 255,
         CGFloat( h        & 0xff) / 255, a)
}

// ── background ────────────────────────────────────────────────────────────────
// macOS icon corner radius ≈ 22.37 % of width (Big Sur spec)
let radius = W * 0.2237
let bgRect  = CGRect(x: 0, y: 0, width: W, height: H)
let bgPath  = CGPath(roundedRect: bgRect, cornerWidth: radius, cornerHeight: radius, transform: nil)

ctx.saveGState()
ctx.addPath(bgPath)
ctx.clip()

// Base fill — Dracula Pro background
ctx.setFillColor(hex(0x22212c))
ctx.fill(bgRect)

// Subtle top-center radial glow (slightly lighter purple tint)
let gColors = [hex(0x3d3654, 0.55), hex(0x22212c, 0.0)] as CFArray
if let grad = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                          colors: gColors,
                          locations: [0, 1] as [CGFloat]) {
    ctx.drawRadialGradient(
        grad,
        startCenter: CGPoint(x: W * 0.50, y: H * 0.36),
        startRadius: 0,
        endCenter:   CGPoint(x: W * 0.50, y: H * 0.50),
        endRadius:   W * 0.68,
        options: []
    )
}

// Thin horizontal accent line at ~62 % height
let lineY   = H * 0.623
let lineH   = H * 0.006
let lineRect = CGRect(x: W * 0.08, y: lineY, width: W * 0.84, height: lineH)
let lineColors = [hex(0x9580ff, 0.0),
                  hex(0x9580ff, 0.45),
                  hex(0x9580ff, 0.45),
                  hex(0x9580ff, 0.0)] as CFArray
if let lg = CGGradient(colorsSpace: CGColorSpaceCreateDeviceRGB(),
                        colors: lineColors,
                        locations: [0, 0.25, 0.75, 1] as [CGFloat]) {
    let lineClip = CGPath(rect: lineRect, transform: nil)
    ctx.saveGState()
    ctx.addPath(lineClip)
    ctx.clip()
    ctx.drawLinearGradient(lg,
        start: CGPoint(x: W * 0.08, y: lineY),
        end:   CGPoint(x: W * 0.92, y: lineY),
        options: [])
    ctx.restoreGState()
}

ctx.restoreGState()

// ── ">" chevron ───────────────────────────────────────────────────────────────
//   Centred slightly left; drawn as two line segments with rounded caps.
let sw: CGFloat = W * 0.056    // stroke width
let chevX: CGFloat = W * 0.300  // tip x
let chevY: CGFloat = H * 0.460  // vertical center
let arm:   CGFloat = H * 0.148  // half-height of each arm
let run:   CGFloat = W * 0.110  // horizontal run

ctx.saveGState()
ctx.addPath(bgPath); ctx.clip()           // keep strokes inside rounded rect

ctx.setLineCap(.round)
ctx.setLineJoin(.round)
ctx.setLineWidth(sw)
ctx.setStrokeColor(hex(0x9580ff))

let chev = CGMutablePath()
chev.move   (to: CGPoint(x: chevX,       y: chevY - arm))
chev.addLine(to: CGPoint(x: chevX + run, y: chevY))
chev.addLine(to: CGPoint(x: chevX,       y: chevY + arm))
ctx.addPath(chev)
ctx.strokePath()

// Soft purple glow behind the chevron (wider, semi-transparent stroke)
ctx.setLineWidth(sw * 2.8)
ctx.setStrokeColor(hex(0x9580ff, 0.18))
ctx.addPath(chev)
ctx.strokePath()

ctx.restoreGState()

// ── blinking cursor block ─────────────────────────────────────────────────────
//   Positioned to the right of where the prompt text would end.
let curX: CGFloat  = W * 0.530
let curY: CGFloat  = H * 0.383
let curW: CGFloat  = W * 0.190
let curH: CGFloat  = H * 0.160
let curRect = CGRect(x: curX, y: curY, width: curW, height: curH)
let curRadius: CGFloat = curW * 0.12
let curPath = CGPath(roundedRect: curRect, cornerWidth: curRadius, cornerHeight: curRadius, transform: nil)

// Glow behind cursor
ctx.saveGState()
ctx.setShadow(offset: .zero, blur: W * 0.055, color: hex(0x9580ff, 0.7))
ctx.setFillColor(hex(0x9580ff, 0.0))  // transparent fill — shadow only
ctx.addPath(curPath)
ctx.fillPath()
ctx.restoreGState()

// Cursor fill (slightly transparent so glow reads through)
ctx.saveGState()
ctx.addPath(bgPath); ctx.clip()
ctx.setFillColor(hex(0x9580ff, 0.92))
ctx.addPath(curPath)
ctx.fillPath()
ctx.restoreGState()

// ── export ────────────────────────────────────────────────────────────────────
let outPath = CommandLine.arguments.count > 1
    ? CommandLine.arguments[1]
    : "assets/AppIcon.png"

guard let img  = ctx.makeImage() else { fatalError("makeImage failed") }
let url  = URL(fileURLWithPath: outPath)
guard let dest = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil)
    else { fatalError("CGImageDestination failed for \(outPath)") }
CGImageDestinationAddImage(dest, img, nil)
guard CGImageDestinationFinalize(dest) else { fatalError("Finalize failed") }

print("Icon written → \(outPath)  (\(px)×\(px) px)")
