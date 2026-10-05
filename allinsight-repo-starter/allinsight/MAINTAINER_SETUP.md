# Maintainer setup

A one-time checklist for going public. Work through it top to bottom. Delete this file once everything is done, or keep it for future maintainers.

## 1. Fill in the placeholders

```powershell
.\scripts\fill-placeholders.ps1 -Owner your-github-name -FullName "Your Full Name" -Domain your-domain.com
```

This replaces `OWNER`, `FULL_NAME` and `YOUR-DOMAIN` everywhere except the license text. Then set up the email aliases the docs mention: `security@`, `privacy@`, `conduct@` and `trademarks@` (they can all forward to one inbox).

## 2. Create the repository

1. Turn on two-factor authentication for your GitHub account (required by SignPath and OpenSSF).
2. Create a **public** repository named `allinsight` and push this folder.
3. **Settings → Code security:** enable private vulnerability reporting, Dependabot alerts, Dependabot security updates, secret scanning and push protection.
4. **Settings → Rules → Rulesets** for `main`: require a pull request, require status checks (CI) to pass, block force pushes, and require signed commits if you sign yours.
5. **Settings → General:** enable Discussions.
6. Pin every GitHub Action to a full commit SHA (`uses: actions/checkout@<sha> # v4`). Scorecard checks for this, and Dependabot will keep the pins updated.

## 3. Free trust signals, in order

| Step | Where | What you can show afterwards |
|---|---|---|
| OSI license | Already done (GPL-3.0-or-later) | "Open source under GPL-3.0" |
| REUSE compliance | Run `pipx run reuse lint` locally; CI checks it | REUSE badge (already in the README) |
| OpenSSF Scorecard | Runs automatically from `.github/workflows/scorecard.yml` | Scorecard badge (already in the README) |
| OpenSSF Best Practices | Register at https://www.bestpractices.dev and answer the questionnaire | "Passing" badge. Put the project ID into the README. |
| Build provenance | Automatic on every tagged release | "Verify with `gh attestation verify`" |
| Software Heritage archive | Submit the repo URL at https://archive.softwareheritage.org/save/ | "Archived by Software Heritage" |
| Microsoft Store | Free individual developer account at https://storedeveloper.microsoft.com | Official "Get it from Microsoft" badge |
| winget | Pull request to https://github.com/microsoft/winget-pkgs after the first release | `winget install` command on the website |
| VirusTotal | Upload each release file at https://www.virustotal.com | Link to each release's scan report (not a "virus-free" seal) |
| Antivirus false positives | Submit releases to Microsoft (https://www.microsoft.com/wdsi/filesubmission) and other vendors if flagged | Nothing to display; fewer warnings for users |
| SignPath Foundation | Apply at https://signpath.org once you have a few public releases and some community activity | Signed installers; activate the line in CODE_SIGNING_POLICY.md |

Optional personal credential: the free OpenSSF course "Developing Secure Software" (LFD121) from the Linux Foundation, for the About page.

## 4. Never display

- "Microsoft Certified", "Approved by Microsoft", or any antivirus vendor logo
- "100% virus-free certified" or "SSL secured" seals
- ISO 27001, SOC 2, "GDPR certified" or "DPDP certified" badges
- ® before the trademark is registered (use ™)
- Download counts, user counts or testimonials that aren't real

## 5. Before the first release

- [ ] Every rule in `rules/cleanup-rules.toml` tested on Windows 10 and Windows 11, including OneDrive-redirected folders, symbolic links and junctions
- [ ] `CLEANUP_RULES.md` matches the TOML file
- [ ] Trademark search for "AllInsight" done in India (classes 9 and 42), the EU and the US
- [ ] Website serves `/.well-known/security.txt` (template in `website/.well-known/`)
- [ ] `security.txt` `Expires` date is less than a year away
