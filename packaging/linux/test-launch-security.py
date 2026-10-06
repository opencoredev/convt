#!/usr/bin/env python3
"""Exercise a relocated RPATH bundle, launcher isolation, and real conversions."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


def run(argv, env, cwd):
    result = subprocess.run([str(a) for a in argv], env=env, cwd=cwd,
                            capture_output=True, text=True, timeout=90)
    if result.returncode:
        raise RuntimeError(f'{argv}: exit {result.returncode}\n{result.stdout}{result.stderr}')
    return result.stdout


def main():
    if len(sys.argv) != 2:
        sys.exit('usage: test-launch-security.py BUNDLE_WITH_RPATH')
    if os.geteuid() == 0:
        sys.exit('run this test as a nonroot user')
    original = Path(sys.argv[1]).resolve()
    with tempfile.TemporaryDirectory(prefix='convt-launch-security-') as tmp:
        work = Path(tmp)
        bundle = work / 'relocated bundle'
        bundle.mkdir()
        for name in ['convt', 'convt.bin', 'ffmpeg', 'ffprobe']:
            shutil.copy2(original / name, bundle / name)
        shutil.copytree(original / 'lib', bundle / 'lib')
        hostile = work / 'hostile'
        hostile.mkdir()
        marker = work / 'helper-ran'
        for name in ['readlink', 'dirname', 'basename']:
            helper = hostile / name
            helper.write_text(f'#!/bin/sh\n/usr/bin/touch "{marker}"\nexit 99\n')
            helper.chmod(0o755)
        env = dict(os.environ)
        for key in ['CONVT_PDFIUM_DIR', 'CONVT_LIBHEIF_DIR', 'CONVT_LIBHEIF_PLUGIN_DIR',
                    'CONVT_FFMPEG', 'CONVT_FFPROBE', 'CONVT_SOFFICE', 'LD_PRELOAD', 'LD_AUDIT']:
            env.pop(key, None)
        env.update(LD_LIBRARY_PATH=f'.:{hostile}:', CONVT_LICENSE_STORE='file',
                   CONVT_CONFIG_DIR=str(work / 'cfg'), CONVT_DATA_DIR=str(work / 'data'),
                   PATH=f'{hostile}:/usr/bin:/bin')
        # Verify executable identity and transitive dlopen dependencies with the
        # exact launcher and proposed main-executable linker flags.
        (work / 'leaf.c').write_text('int leaf(void) { return 42; }\n')
        (work / 'private.c').write_text('int leaf(void); int value(void) { return leaf(); }\n')
        (work / 'probe.c').write_text(r'''#include <dlfcn.h>
#include <stdlib.h>
#include <unistd.h>
#include <string.h>
int main(int argc, char **argv) {
  char path[4096]; ssize_t n = readlink("/proc/self/exe", path, sizeof(path)-1);
  if (n < 0 || argc != 2 || getenv("LD_LIBRARY_PATH")) return 1;
  path[n] = 0; if (strcmp(path, argv[1])) return 2;
  void *lib = dlopen("libconvt_private.so", RTLD_NOW);
  if (!lib) return 3;
  int (*value)(void) = dlsym(lib, "value");
  return !value || value() != 42;
}
''')
        run(['cc', '-shared', '-fPIC', work / 'leaf.c', '-o', bundle / 'lib/libconvt_leaf.so'], env, work)
        run(['cc', '-shared', '-fPIC', work / 'private.c', '-L', bundle / 'lib',
             '-lconvt_leaf', '-o', bundle / 'lib/libconvt_private.so'], env, work)
        run(['cc', work / 'probe.c', '-ldl', '-Wl,--disable-new-dtags', '-Wl,-rpath,$ORIGIN/lib',
             '-o', bundle / 'probe.bin'], env, work)
        shutil.copy2(bundle / 'convt', bundle / 'probe')
        run([bundle / 'probe', bundle / 'probe.bin'], env, hostile)
        assert not marker.exists(), 'launcher executed a helper from PATH'
        print('PASS: nonroot relocation, executable identity, private transitive dlopen, absolute helpers')
        real_libm = Path(subprocess.check_output(['/usr/bin/cc', '-print-file-name=libm.so.6'], text=True).strip()).resolve()
        assert real_libm.is_file()
        (hostile / 'libm.so.6').symlink_to(real_libm)
        trace = subprocess.run([str(bundle / 'convt'), 'engines'], env=dict(env, LD_DEBUG='libs'),
                               cwd=hostile, capture_output=True, text=True, timeout=90)
        assert trace.returncode == 0, trace.stderr
        assert '/libm.so.6' in trace.stderr, 'loader trace did not resolve libm'
        assert str(hostile / 'libm.so.6') not in trace.stderr and './libm.so.6' not in trace.stderr, trace.stderr
        print('PASS: loader tracing rejects working-directory libm with inherited LD_LIBRARY_PATH=.')
        engines = run([bundle / 'convt', 'engines'], env, hostile).splitlines()
        for engine in ['ffmpeg', 'pdfium', 'libheif', 'libreoffice']:
            assert engine in engines, f'{engine} is unavailable: {engines}'
        # Hide system FFmpeg while probing discovery; current_exe must locate
        # the bundled executable even from a different working directory.
        isolated = dict(env, PATH=str(hostile))
        assert 'ffmpeg' in run([bundle / 'convt', 'engines'], isolated, hostile).splitlines()
        print('PASS: bundled native engines and FFmpeg discovery without system PATH')
        run([bundle / 'ffmpeg', '-v', 'error', '-f', 'lavfi', '-i',
             'sine=frequency=440:duration=0.2', work / 'tone.wav'], env, hostile)
        run([bundle / 'convt', work / 'tone.wav', '--to', 'mp3', '-o', work / 'audio'], isolated, hostile)
        info = run([bundle / 'ffprobe', '-v', 'error', '-show_entries', 'stream=codec_name,sample_rate',
                    '-of', 'default=noprint_wrappers=1', work / 'audio/tone.mp3'], env, hostile)
        assert 'codec_name=mp3' in info and 'sample_rate=44100' in info, info
        print('PASS: bundled FFmpeg produced independently probed MP3')
        office = work / 'office'
        office.write_text(f'''#!/bin/sh
[ "${{LD_LIBRARY_PATH+x}}" != x ] || exit 98
[ "${{LIBHEIF_PLUGIN_PATH:-}}" != "{bundle}/lib/libheif/plugins" ] || exit 97
/usr/bin/touch "{work / 'office-child-clean'}"
exec /usr/bin/soffice "$@"
''')
        office.chmod(0o755)
        env['CONVT_SOFFICE'] = str(office)
        # System Office's shell wrapper uses ordinary PATH helpers of its own.
        env['PATH'] = '/usr/bin:/bin'
        (work / 'document.txt').write_text('Convt isolated Office test\n')
        run([bundle / 'convt', work / 'document.txt', '--to', 'pdf', '-o', work / 'documents'], env, hostile)
        assert (work / 'office-child-clean').exists(), 'Office child environment not checked'
        pdf = work / 'documents/document.pdf'
        assert pdf.read_bytes().startswith(b'%PDF-'), 'Office did not write a PDF'
        run([bundle / 'convt', pdf, '--to', 'png', '-o', work / 'pages'], env, hostile)
        assert (work / 'pages/document.png').read_bytes().startswith(b'\x89PNG\r\n\x1a\n')
        print('PASS: clean system Office child produced PDF; bundled PDFium rendered PNG')


if __name__ == '__main__':
    main()
