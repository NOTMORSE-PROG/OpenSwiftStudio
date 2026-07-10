# Known Limitations

This document lists known limitations and compatibility caveats in OpenSwiftStudio. It is expanded over the course of development (the full limitations matrix is tracked as ticket DOC-2); the sections below are the ones that are load-bearing today.

## Swift toolchain compatibility

OpenSwiftStudio builds your app with the official Swift toolchain for Windows (downloaded and installed by the setup wizard). A Swift toolchain can install successfully and still fail to **compile** on a given machine. There are two distinct causes:

1. **Version-specific compiler bugs.** Some Swift releases have crashed during compilation on Windows regardless of hardware. For example, the Swift 6.3.x line crashes with an illegal-instruction fault inside the toolchain's own `Foundation` library while compiling — on every Windows machine, including current high-end CPUs. This is why OpenSwiftStudio pins a Swift version that is verified to compile and run a sample project, and only moves the pin forward after re-verifying with a real build (not just a version check).
2. **Unsupported CPU instructions on older processors.** Optimized compiler binaries can use CPU instructions that older or low-end processors do not implement, which also surfaces as an illegal-instruction crash. This one *is* hardware-specific and mainly affects older machines.

### How OpenSwiftStudio protects you

- **Compile self-test after install.** When the setup wizard finishes installing Swift, it builds a tiny throwaway package on your actual machine. If the toolchain crashes, the wizard tells you clearly — "Swift installed, but it crashed compiling on this machine (this is not your code)" — instead of letting you discover it later as a mysterious failure. Because the self-test runs on *your* hardware, it covers every machine, not just the ones we tested on.
- **Distinct Run-time message.** If a build crashes the toolchain (as opposed to a normal compile error in your code), the Console labels it as a toolchain crash, not a build failure in your project.
- **Conservative version pinning.** The bundled Swift version is chosen for verified stability, and the project re-evaluates newer releases with a real compile test before adopting them.

### What to do if you hit a toolchain crash

- Check the hardware requirements in the project README and confirm your CPU and Windows version meet the minimums.
- Try a different Swift version if one is available (a future release may fix a version-specific bug).
- Report it: an illegal-instruction crash while compiling a trivial program is a Swift toolchain issue worth reporting upstream, and worth telling us about so we can adjust the pinned version.

### Honesty note

The bundled toolchain version is validated on a limited set of hardware. The compile self-test above is the primary safeguard for the wider range of machines real users run — it validates the toolchain on each user's own hardware at setup time.
