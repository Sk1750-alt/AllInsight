# Privacy and the updater

> **AllInsight's update mechanism does not upload user analytics, documents,
> datasets, reports, AI interactions, or personal data.**

The general privacy statement is in [`../PRIVACY.md`](../PRIVACY.md). This page
covers the one feature that uses the network.

## When AllInsight goes online

It goes online in only two situations, and both require the user:

- **You click *Check for Updates*** in Settings → Updates.
- **You chose *Automatically check for updates*.** The default is *Ask me
  before checking*. With automatic checks on, AllInsight checks at most once a
  day, never in the first ten minutes after launch, and stops as soon as you
  switch the option off.

Downloading an update only happens when you click **Update Now**, and
installing it only when you click **Restart Now**. Nothing installs silently,
and AllInsight never restarts without your consent.

## What is sent

A check makes two HTTPS `GET` requests to fixed addresses:

```text
GET https://github.com/Sk1750-alt/AllInsight/releases/latest/download/latest.json
GET https://github.com/Sk1750-alt/AllInsight/releases/latest/download/latest.json.sig
User-Agent: AllInsight-Updater
```

The file is the same for every user. It lists every platform, and your copy
of AllInsight picks its own entry after downloading it, so the request
doesn't need to say what you run. A download is one more `GET` for the package
file named in that list.

Requests contain none of the following, and a test (`requests_contain_nothing_about_the_user_or_machine`) checks this:

- your user name, e-mail address, or any account (AllInsight has none)
- an installation ID, device ID or fingerprint (none is ever created)
- your AllInsight version, operating system, or hardware
- cookies (the HTTP client is built without cookie support)
- file names, file contents, scan results, cleanup history, settings,
  database contents, assistant questions or answers, usage history, or
  analytics of any kind (the updater module cannot reach any of these)

The update server, which is GitHub Releases by default, sees the connection
itself, as any web server does: your IP address and the time of the request.
AllInsight doesn't add your IP address to anything. Under GitHub's own privacy
policy, GitHub can see that this IP address downloaded a public file.

## What is stored locally

These items are stored on your device and are never sent anywhere:
- your update preference and interval, in the local settings document
- the time of the last successful check (`update.last_check`)
- `logs/updates.log`, which holds the updater's own messages
- a verified package in `updates/` until it has been installed
- database backups in `backups/`

No update setting is stored on a server.

## No telemetry

The updater has no analytics, crash reporting, third-party SDKs, A/B
assignment, staged-rollout identifiers or installation counters. A new
release is offered to everyone who checks, and AllInsight cannot know who did.
