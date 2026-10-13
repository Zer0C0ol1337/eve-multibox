# Code signing with SignPath (free for open source)

Windows releases of EVE Multibox can be signed for free through the
[SignPath Foundation](https://signpath.org/). A signed `.exe` usually triggers fewer antivirus false positives and
SmartScreen warnings. Until signing is set up, the release workflow simply builds unsigned binaries.

## 1. Apply

1. Enable two-factor authentication on GitHub (required by SignPath).
2. Apply at <https://signpath.org/apply> with the repository URL
   `https://github.com/Zer0C0ol1337/eve-multibox`.
3. The README already contains the required **code signing policy** (roles, privacy statement).

## 2. Configure SignPath (after approval)

In the SignPath web app:

- **Project** with slug `eve-multibox`, linked to the GitHub repository (trusted build system: GitHub.com).
- **Signing policy** with slug `release-signing` (certificate from SignPath Foundation, manual approval by you).
- **Artifact configuration** (slug `initial`, or set `SIGNPATH_ARTIFACT_CONFIG_SLUG`). The workflow uploads a
  GitHub artifact (a ZIP) that contains only `eve-multibox.exe`:

```xml
<?xml version="1.0" encoding="utf-8"?>
<artifact-configuration xmlns="http://signpath.io/artifact-configuration/v1">
  <zip-file>
    <pe-file path="eve-multibox.exe" product-name="EVE Multibox">
      <authenticode-sign/>
    </pe-file>
  </zip-file>
</artifact-configuration>
```

- Create an **API token** for a CI user that may submit signing requests for this project.

## 3. Configure GitHub

Repository → **Settings → Secrets and variables → Actions**:

| Type | Name | Value |
|---|---|---|
| Secret | `SIGNPATH_API_TOKEN` | the API token from SignPath |
| Variable | `SIGNPATH_ORGANIZATION_ID` | your SignPath organization ID |
| Variable (optional) | `SIGNPATH_PROJECT_SLUG` | only if not `eve-multibox` |
| Variable (optional) | `SIGNPATH_POLICY_SLUG` | only if not `release-signing` |
| Variable (optional) | `SIGNPATH_ARTIFACT_CONFIG_SLUG` | only if not `initial` |

As soon as `SIGNPATH_ORGANIZATION_ID` is set, the release workflow signs the `.exe` before packaging it.

## 4. Releasing with signing

1. Publish a release as usual (see README → "Publishing a release").
2. The Windows job uploads the unsigned `.exe` to SignPath and waits (up to 60 minutes).
3. **Approve the signing request** in SignPath (you get an e-mail) – the job then downloads the signed `.exe`,
   packages the ZIP and attaches it to the release.
