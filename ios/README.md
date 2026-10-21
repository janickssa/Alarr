# Quorum for iPhone

Native SwiftUI app (iOS 17+) with widgets. It talks only to your Quorum server, never to
the services themselves.

The same sources build two ways: **on Linux** with xtool through the Forgejo `ios` runner
(the main route, straight to TestFlight), or **on a Mac** with Xcode and XcodeGen.

```
ios/
├─ Package.swift          Linux build: the app and widget products for xtool
├─ xtool.yml              Linux build: bundle ids, Info.plists, entitlements
├─ AppConfig/             Linux build: literal Info.plists and entitlements
├─ project.yml            Mac build: XcodeGen spec (the Xcode project is generated, not committed)
├─ Config/Shared.xcconfig Mac build: bundle id, app group, versions; your values go in Local.xcconfig
├─ QuorumKit/             Swift package shared by the app and the widgets
│  ├─ Sources/QuorumAPI   API client generated from openapi/openapi.json at build time
│  └─ Sources/QuorumCore  Keychain, live events (SSE), widget cache, formatting, colours
├─ Quorum/                the app
└─ QuorumWidgets/         widget extension
```

## Build it on Linux (Forgejo → TestFlight)

Every push to `main` that touches `ios/` or the API spec runs `.forgejo/workflows/ios.yml`
on the self-hosted `ios` runner through the shared `jss/ios-ship@v1` action: QuorumKit's
tests (`swift test`), an xtool release build, App Store signing, offline validation and the
upload to TestFlight. *Run workflow* in Forgejo can build without uploading.

| | |
| --- | --- |
| App | `dk.schwabejss.quorum` |
| Widgets | `dk.schwabejss.quorum.widgets` |
| App Group | `group.dk.schwabejss.quorum` |
| Marketing version | `CFBundleShortVersionString` in both `AppConfig/*-Info.plist` (keep them equal); `ship.sh` stamps the build number |

Values that Xcode would fill in from build settings are written out literally in
`AppConfig/`, since xtool doesn't expand `$(…)`. Push uses the production APNs environment
(release builds), so set `environment = "production"` under `[apns]` on the server.

### Apple-side setup (once, in the browser)

`ship.sh` registers the two bundle ids and creates their App Store profiles itself, but
it can't create App Groups, enable capabilities or create the app record:

- [ ] Identifiers → App Groups → **+** → `group.dk.schwabejss.quorum`
- [ ] App ID `dk.schwabejss.quorum`: enable **Push Notifications**, **App Groups** (tick
      the group), **Time Sensitive Notifications**
- [ ] App ID `dk.schwabejss.quorum.widgets`: enable **App Groups** (tick the group)
- [ ] App Store Connect → Apps → **+** → New App with bundle id `dk.schwabejss.quorum`
- [ ] TestFlight: internal group with automatic distribution
- [ ] APNs key and server config: steps 4–5 under *Distribution* below, with
      `bundle_id = "dk.schwabejss.quorum"`

If the App IDs don't exist yet when you do this, run the workflow once (it registers them
and then fails at signing or upload), or register them by hand.

## Build it on a Mac (Xcode)

You need a Mac with **Xcode 26** or newer (the API generator needs Swift 6.2) and
[XcodeGen](https://github.com/yonaskolb/XcodeGen).

```sh
brew install xcodegen
cd ios
cp Config/Local.xcconfig.example Config/Local.xcconfig   # then edit it
xcodegen generate
open Quorum.xcodeproj
```

`Config/Local.xcconfig` holds your Apple team id and bundle id and is not committed:

```
DEVELOPMENT_TEAM = ABCDE12345
QUORUM_BUNDLE_ID = com.yourname.quorum
```

The first build asks you to **trust the OpenAPIGenerator plug-in** (Xcode shows "Trust &
Enable"). It generates the Swift client from `openapi/openapi.json` on every build, so the
app always matches the server's API. Run `xcodegen generate` again after pulling changes
to `project.yml`.

## Connect to your server

The app needs HTTPS to reach Quorum (iOS blocks plain HTTP except to local-network
addresses). On a tailnet, `tailscale serve` or a `tailscale cert` certificate gives the
server an HTTPS address; set `[server] public_url` in `quorum.toml` to it. That address is
in the pairing QR code.

Pair the phone: in Quorum on your computer open **Settings → Users → Pair a device**, then
in the app tap **Scan QR code** (or type the server address and the 8-character code).
The device token is stored in the Keychain; revoking the device in Settings signs the app
out.

## Distribution (paid Apple Developer account)

Install through **TestFlight** or as an **ad hoc** build; both avoid the 7-day expiry of
free provisioning.

1. **App ID** — developer.apple.com → Certificates, IDs & Profiles → Identifiers → **+** →
   App IDs → App. Bundle ID: the `QUORUM_BUNDLE_ID` you chose. Capabilities: **Push
   Notifications**, **App Groups**, **Time Sensitive Notifications**.
2. **Widget App ID** — the same with `<bundle id>.widgets` and the **App Groups** capability.
3. **App Group** — Identifiers → App Groups → **+** → `group.<bundle id>`. Assign it to both
   App IDs (Edit → App Groups → Configure). With automatic signing Xcode does steps 1–3 for
   you the first time you build with your team selected.
4. **APNs key** — Keys → **+** → name it, tick **Apple Push Notifications service (APNs)**,
   download `AuthKey_<KEY ID>.p8` (you can download it only once). Note the Key ID and
   your Team ID (top right of the developer site).
5. **Server** — copy the key to the Quorum server and point the config at it:

   ```sh
   sudo install -m 600 -o quorum -g quorum AuthKey_XYZ987WVUT.p8 /etc/quorum/
   ```

   ```toml
   [apns]
   enabled = true
   team_id = "ABCDE12345"
   key_id = "XYZ987WVUT"
   key_path = "/etc/quorum/AuthKey_XYZ987WVUT.p8"   # mode 600, never commit it
   bundle_id = "com.yourname.quorum"
   environment = "production"
   ```

   Restart Quorum. **Settings → Alerts** then lists "iOS push" as set up.
6. **TestFlight** — in Xcode choose *Any iOS Device*, **Product → Archive**, then
   **Distribute App → TestFlight & App Store**. Create the app record in App Store Connect
   when Xcode asks. Install TestFlight on the iPhone and accept the build.
   **Ad hoc** instead: register the iPhone's UDID under Devices, then **Distribute App →
   Release Testing** and install the `.ipa` with Apple Configurator or Finder.

Each phone tells the server which APNs environment its token belongs to: builds run from
Xcode use the sandbox, TestFlight and ad hoc builds use production. Tokens that APNs
rejects are removed automatically; every push delivery is in the activity log.

## Notifications

After pairing the app asks for permission and registers its push token. In **Settings →
Alerts** (web) choose per alert type whether it goes to ntfy, push or both, and quiet hours;
the app shows the same settings under **Settings → Notifications**, plus mutes that apply
only to this iPhone.

- "Service down" and "Node offline" are Time Sensitive (they break through Focus).
- A new Seerr request has **Approve** and **Decline** buttons.
- A down service has **Open**. Power actions are never offered from a notification.
- Tapping a notification opens the matching page.

## Widgets

Small (problem count or one service's number), medium (downloads or streams), large (nodes
and problems), and lock screen (download speed gauge, problem count). The app writes the
latest summary to the App Group; widgets show it and refresh it themselves every 15
minutes using the same Keychain token.

## App icon

`Quorum/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png` is rendered from the design by
`web/scripts/pwa-assets.mjs` (as JPEG) and converted to a PNG without alpha:

```sh
cd web && NODE_PATH=$(npm root -g) node scripts/pwa-assets.mjs
cd ../ios && python3 -c "from PIL import Image; Image.open('app-icon-1024.jpg').convert('RGB').save('Quorum/Assets.xcassets/AppIcon.appiconset/AppIcon-1024.png')" && rm app-icon-1024.jpg
```

## CI

`.forgejo/workflows/ios.yml` (Forgejo, Linux) builds, signs and uploads to TestFlight; see
*Build it on Linux* above.

`.github/workflows/ios.yml` (GitHub) runs on macOS when `ios/` or the API spec changes: QuorumKit
unit tests, then an unsigned simulator build of the app and widgets.
