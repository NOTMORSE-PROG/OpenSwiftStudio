# Legal — OpenSwiftStudio

This document captures OpenSwiftStudio's licensing, clean-room rules, trademark guidance, and the legal precedents underpinning the project. The rules here are binding on contributors.

## 1. License — Apache-2.0

OpenSwiftStudio is licensed under the [Apache License 2.0](../LICENSE). This is permanent and binding: no paid tier, no premium edition, no upsell. Anyone may use, modify, redistribute, and ship products built with OpenSwiftStudio, including for commercial purposes.

The Apache-2.0 license includes a patent grant from contributors and requires preservation of copyright and license notices in derivative works. See the LICENSE file for the full text.

## 2. Clean-room rules (binding)

OpenSwiftStudio is a clean-room reimplementation of the iOS development workflow on Windows. The rules below are absolute and may not be relaxed without legal review:

**The project never:**
- Bundles, redistributes, or ships Apple binaries, libraries, frameworks, or runtime files
- Copies Apple source code, header files, or interface definitions into this repository
- Includes Apple graphic assets — system icons, glyphs, SF Symbols, Apple-designed UI artwork
- Uses Apple-trademarked names or marks as the project's identity (see §3)

**The project does:**
- Reimplement public Apple APIs from scratch, sourcing only from Apple's public developer documentation (developer.apple.com)
- Reference behavior described in published Apple documentation, WWDC session transcripts, and other public materials
- Depend on third-party open-source projects (SwiftCrossUI, forked Cacao, xtool, Tauri, Monaco, Codicons) under their own licenses
- Require users to fetch Apple-distributed components (Xcode XIP, Swift toolchain) directly from Apple under the user's own Apple Developer account — OpenSwiftStudio never re-hosts these

This separation is what allows the project to exist legally. Violating it would expose the project — and downstream users — to copyright and trade-dress claims that the precedents in §4 do not cover.

## 3. Trademark guidance

**Project naming.** "OpenSwiftStudio" is a working name chosen to avoid Apple trademarks. Per the project's planning docs, the name deliberately:

- Does **not** start with a lowercase "i" (avoids the iOS / iPad / iPhone visual lineage)
- Does **not** include "Apple", "Mac", "iOS", "iPhone", "iPad", "watchOS", "tvOS", "visionOS", "Xcode", or "Swift Playgrounds" as part of the product name
- Uses "Swift" descriptively (Swift is an open-source language under Apache-2.0 stewarded at swift.org; descriptive use of the language name in a tooling product is established practice — e.g., SwiftLint, SwiftPM, SwiftCrossUI)

**Descriptive use is fine.** Documentation, marketing copy, README, and code comments may freely refer to "iOS", "iPhone", "iPad", "Xcode", "App Store", "ARKit", "RealityKit", and other Apple terms when used descriptively to explain what the project does or interoperates with. Nominative use ("compatible with iOS") is protected. Confusing use ("an Apple product", branded screenshots) is not.

**Code and asset boundaries.** Per §2, the project never ships Apple-trademarked logos, the Apple logo, the Xcode hammer icon, SF Symbols, system app icons, or device frame artwork derived from Apple's industrial design. Device frames shipped by OpenSwiftStudio (under `runtime/openswift-deviceframes/`) are independent illustrations of generic phone/tablet hardware silhouettes, not Apple-property reproductions.

**If the working name needs to change.** "OpenSwiftStudio" is a working name and may be revised before a public 1.0 release if trademark research surfaces a conflict. Renaming is non-blocking — the technology and license are the project; the name is a label.

## 4. Legal precedent

The clean-room reimplementation approach taken by OpenSwiftStudio rests on settled and well-tested legal ground. None of the projects below have been sued out of existence for reimplementing closed platforms in the open:

- **Google v Oracle America (2021)** — The U.S. Supreme Court ruled that Google's reimplementation of the Java SE API in Android was fair use. The ruling specifically addressed API reimplementation as a transformative activity that does not infringe copyright when the implementing code is independently written. ([decision summary](https://en.wikipedia.org/wiki/Google_LLC_v._Oracle_America,_Inc.))
- **Wine** — 30+ years of clean-room Win32 API reimplementation on Linux/macOS/BSD. ([winehq.org](https://www.winehq.org/))
- **GNUstep** — 25+ years of clean-room Cocoa / OpenStep reimplementation, predates and outlasts every Apple developer-tools generation. ([gnustep.org](http://www.gnustep.org/))
- **Darling** — Reimplements macOS userland on Linux (similar in spirit to Wine for Windows). ([darlinghq.org](https://www.darlinghq.org/))
- **ReactOS** — 25+ years of clean-room Windows NT kernel and userland reimplementation. ([reactos.org](https://reactos.org/))
- **Skip** — Active commercial project translating Swift/SwiftUI to Kotlin/Jetpack Compose for Android — under active development, not under legal challenge. ([skip.tools](https://skip.tools/))
- **SwiftCrossUI** — Active SwiftUI-API-compatible UI runtime on non-Apple platforms (vendored by OpenSwiftStudio). ([swift-cross-ui](https://github.com/stackotter/swift-cross-ui))
- **xtool** — Active project for deploying iOS apps from Linux/Windows (vendored by OpenSwiftStudio for real-device deploy). ([xtool-org/xtool](https://github.com/xtool-org/xtool))

These projects collectively demonstrate that reimplementing Apple's APIs from public documentation, distributing the implementation as open source, and never bundling Apple property is a settled and defensible practice.

## 5. What this means for users

**The IDE itself never ships Apple property.** OpenSwiftStudio does not contain or redistribute Xcode, the iOS SDK, Apple's Swift toolchain binaries, or any Apple-licensed material. The setup wizard (M0.5) directs users to download these from Apple under the user's own Apple Developer account — the same way every other tool that interoperates with the Apple ecosystem does.

**Distribution still goes through Apple.** When users distribute their own apps to the App Store, they do so under Apple's standard developer agreement, which includes:
- An Apple Developer Program membership ($99/yr, paid directly to Apple — never to OpenSwiftStudio)
- App Review and Apple's own distribution channels
- Apple's content policies and platform guidelines

OpenSwiftStudio has no role in App Store distribution beyond producing build artifacts the user submits themselves. Per the layered roadmap, an optional v0.3 "Cloud Mac runner" path will help users produce App Store-ready binaries via third-party cloud Mac providers (GitHub Actions macOS runners, MacStadium, etc.) — those costs go directly to the third party, never to OpenSwiftStudio.

**Free-tier device deployment.** OpenSwiftStudio's M9 milestone supports deploying to a user's own iPhone via xtool over USB (WSL2 + usbipd-win). This uses Apple's free-tier developer signing path — a free Apple ID is sufficient; the $99/yr membership is only required for App Store distribution. Apple's free-tier 7-day signing renewal is handled silently in the background.

**No telemetry, no data collection.** The IDE does not phone home. The Run button never triggers an authentication or network round-trip. Token storage uses Windows Credential Manager (DPAPI). User code, user projects, and user activity stay on the user's machine.

## 6. Reporting concerns

If you believe any part of OpenSwiftStudio infringes a copyright, trademark, or other right, please open a GitHub issue using the project's standard issue templates. Pre-public-release, contact the project maintainer through the channels listed in the repository README.

OpenSwiftStudio takes legal concerns seriously and will investigate any specific, sourced claim. The project does not abandon features in response to vague threats — concerns are evaluated against the clean-room rules in §2 and the precedents in §4.
