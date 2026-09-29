# Releasing EVJ

The public repository gets one snapshot commit per release on `main`; a pushed tag `vX.Y.Z` makes GitHub Actions ([`.github/workflows/release.yml`](../.github/workflows/release.yml)) build the installer and publish the release.

1. Set `version = "X.Y.Z"` in `Cargo.toml` (workspace) and run `cargo build` so `Cargo.lock` follows.
2. Write the release text in `docs/release-notes/vX.Y.Z.md` (shown on the release page; English).
3. Update `README.md` (installer name in *Getting started*, *Status*) and commit.
4. Publish the snapshot and the tag:

   ```powershell
   $sha = git commit-tree "HEAD^{tree}" -p <previous public main> -m "EVJ — Event Visual Jockey vX.Y.Z"
   git tag -a vX.Y.Z $sha -m "EVJ vX.Y.Z"
   git push https://github.com/Imagaa/event-visual-jockey.git "${sha}:refs/heads/main" vX.Y.Z
   ```

5. Actions → **Release** runs (about 30–40 min with a cold cache): `EVJ-Setup-X.Y.Z.exe` and `EVJ-Setup-X.Y.Z.exe.sha256` appear on the release, which becomes the latest. The build fails early if `Cargo.toml` and the tag disagree.

To rebuild an existing tag (e.g. after a fix in the build scripts): Actions → Release → *Run workflow*, enter the tag; untick *publish* to only build (the installer is kept as a workflow artifact for 7 days).

Local build of the same installer: `tools\build-installer.ps1` → `dist\EVJ-Setup-<version>.exe` (needs Inno Setup 6).
