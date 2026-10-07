"""Release workflow layout: one cached build, dispatch-only reproducibility."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = ROOT / '.github' / 'workflows'


def workflow(name):
    return (WORKFLOWS / name).read_text()


class ReleaseWorkflows(unittest.TestCase):
    def test_linux_release_is_one_cached_build(self):
        text = workflow('release-linux.yml')
        self.assertIn('linux-release-review', text)
        self.assertIn('packaging/out/ci-a/${{ inputs.version }}/', text)
        self.assertIn('Swatinem/rust-cache@v2', text)
        self.assertIn('CONVT_RELEASE_BUILD_WORK', text)
        self.assertIn('${{ runner.temp }}/convt-linux-compile', text)
        self.assertNotIn('Rebuild twice', text)
        self.assertNotIn('compare.py', text)
        self.assertNotIn('packaging/out/ci-b', text)
        self.assertIn('cancel-in-progress: true', text)
        self.assertIn('runs-on: ubuntu-24.04', text)
        self.assertNotIn('linux-bundle-\n', text)

    def test_reproducibility_is_dispatch_only(self):
        text = workflow('release-reproducibility.yml')
        self.assertIn('workflow_dispatch:', text)
        self.assertNotIn('schedule:', text)
        self.assertIn('compare.py', text)
        self.assertIn('packaging/out/ci-a', text)
        self.assertIn('packaging/out/ci-b', text)
        self.assertIn('runs-on: ubuntu-24.04', text)
        self.assertIn('runs-on: macos-15', text)
        self.assertIn('runs-on: windows-latest', text)
        self.assertIn('cancel-in-progress: true', text)
        self.assertIn('rm "$key"', text)
        self.assertIn('Remove-Item -Recurse -Force target', text)

    def test_macos_release_stays_arm64(self):
        text = workflow('release-macos.yml')
        self.assertIn('runs-on: macos-15', text)
        self.assertIn('macos-release-review', text)
        self.assertIn('convt-macos-arm64.dmg', text)
        self.assertIn('Swatinem/rust-cache@v2', text)
        self.assertIn('cancel-in-progress: true', text)
        self.assertNotIn('Rebuild twice', text)

    def test_windows_release_is_one_cached_build(self):
        text = workflow('release-windows.yml')
        self.assertIn('runs-on: windows-latest', text)
        self.assertIn('windows-release-review', text)
        self.assertIn('Swatinem/rust-cache@v2', text)
        self.assertIn('cancel-in-progress: true', text)
        self.assertNotIn('Rebuild twice', text)

    def test_umbrella_release_calls_platforms_and_queues(self):
        text = workflow('release.yml')
        self.assertIn('uses: ./.github/workflows/release-linux.yml', text)
        self.assertIn('uses: ./.github/workflows/release-macos.yml', text)
        self.assertIn('uses: ./.github/workflows/release-windows.yml', text)
        self.assertIn('*-release-review', text)
        self.assertIn('tenki-standard-medium-4c-8g', text)
        self.assertIn('tenki-standard-large-8c-16g', text)
        self.assertIn('cancel-in-progress: false', text)
        self.assertNotIn('Swatinem/rust-cache@v2', text)


if __name__ == '__main__':
    unittest.main()
