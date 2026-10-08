# Security Policy

AllInsight runs with access to your file system and can delete files. We treat security reports as the highest priority work in the project.

## Supported versions

| Version | Supported |
|---|---|
| Latest release | ✅ Yes |
| Older releases | ❌ Please update first and check whether the issue still occurs |
| Pre-release / `main` | Best effort |

## Reporting a vulnerability

**Do not open a public issue.** Use one of these private channels:

1. **GitHub private vulnerability reporting:** Security → Report a vulnerability on [this repository](https://github.com/Sk1750-alt/AllInsight/security/advisories/new) (preferred).
2. Open a private discussion or contact the maintainer directly.

Please include the AllInsight version, your OS version, steps to reproduce, and the impact you observed. Proof-of-concept code is welcome; please do not include real personal files.

## What to expect

| Step | Target time |
|---|---|
| Acknowledgement | Within 72 hours |
| Initial assessment and severity | Within 7 days |
| Fix for critical issues | As fast as possible, normally within 14 days |
| Public advisory | After a fixed release is available, credited to you unless you prefer otherwise |

This is a volunteer-maintained project, so these are targets, not guarantees. We will keep you updated if something takes longer.

## Severity guide

**Critical**
- Any way to make AllInsight delete, move or modify a file classified as Protected or Review without explicit user approval.
- Any way for model output, a crafted file name, or file content to trigger an action.
- Escaping the cleanup allowlist through path traversal, symbolic links, junctions, hard links or race conditions.
- Code execution through the installer or AI model loading.

**High**
- Privilege escalation through AllInsight's elevated operations.
- Any network transmission of user data without the user's request.
- Personal file names or contents written to logs or diagnostics exports.

**Medium / Low**
- Denial of service, crashes on malformed input, misleading health readings.

## Out of scope

- Issues requiring an attacker who already has administrator access to the machine.
- Vulnerabilities in third-party GGUF models themselves (report those upstream). Model loading bugs in AllInsight are in scope.
- Reports from automated scanners without a demonstrated impact.

## Safe harbor

We will not pursue legal action against good-faith research that follows this policy, avoids privacy violations and data destruction on systems you don't own, and gives us reasonable time to fix the issue before disclosure.

## Recognition

There is no paid bug bounty yet. Reporters of valid issues are credited in the release notes, if they wish.

### Hall of Fame

_Be the first._
