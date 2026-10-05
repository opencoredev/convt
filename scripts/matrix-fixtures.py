#!/usr/bin/env python3
"""Independent, disposable Office/PDF/HEIC fixtures and semantic Office checks."""
import ctypes as c
import ctypes.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import zipfile

MARKER = 'CONVT_MATRIX_7F3A'
NS = ('xmlns:office="urn:oasis:names:tc:opendocument:xmlns:office:1.0" '
      'xmlns:text="urn:oasis:names:tc:opendocument:xmlns:text:1.0" '
      'xmlns:table="urn:oasis:names:tc:opendocument:xmlns:table:1.0" '
      'xmlns:draw="urn:oasis:names:tc:opendocument:xmlns:drawing:1.0" '
      'xmlns:presentation="urn:oasis:names:tc:opendocument:xmlns:presentation:1.0" '
      'xmlns:style="urn:oasis:names:tc:opendocument:xmlns:style:1.0" xmlns:fo="urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0" xmlns:svg="urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0"')


def office_convert(src, target, dest):
    dest.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='convt-office-profile-') as profile:
        tool = os.environ.get('CONVT_SOFFICE') or shutil.which('soffice') or shutil.which('libreoffice')
        result = subprocess.run([tool, '-env:UserInstallation=' + Path(profile).as_uri(),
                                 '--headless', '--norestore', '--convert-to', target,
                                 '--outdir', str(dest), str(src)], capture_output=True, timeout=60)
        ext = target.split(':')[0]
        outputs = list(dest.glob('*.' + ext))
        if result.returncode or len(outputs) != 1:
            raise RuntimeError(f'Office {src.name} -> {target}: {result.stdout!r} {result.stderr!r}')
        return outputs[0]


def office_fixture(dest, kind):
    bodies = {
        'odt': f'<office:text><text:p>{MARKER}</text:p><text:p>Known document text 12345</text:p></office:text>',
        'odp': f'<office:presentation><draw:page draw:name="Slide1"><draw:frame svg:x="1cm" svg:y="1cm" svg:width="20cm" svg:height="3cm"><draw:text-box><text:p>{MARKER}</text:p></draw:text-box></draw:frame></draw:page></office:presentation>',
        'ods': f'<office:spreadsheet><table:table table:name="KnownCells"><table:table-column table:style-name="wide"/><table:table-column/><table:table-row><table:table-cell office:value-type="string"><text:p>{MARKER}</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="42"><text:p>42</text:p></table:table-cell></table:table-row><table:table-row><table:table-cell office:value-type="string"><text:p>KnownCell</text:p></table:table-cell><table:table-cell office:value-type="float" office:value="3.5"><text:p>3.5</text:p></table:table-cell></table:table-row></table:table></office:spreadsheet>',
    }
    mime = {'odt': 'text', 'odp': 'presentation', 'ods': 'spreadsheet'}[kind]
    path = dest / ('sample.' + kind)
    with zipfile.ZipFile(path, 'w') as z:
        z.writestr('mimetype', 'application/vnd.oasis.opendocument.' + mime)
        z.writestr('content.xml', f'<?xml version="1.0"?><office:document-content {NS} office:version="1.2"><office:automatic-styles><style:style style:name="wide" style:family="table-column"><style:table-column-properties style:column-width="5cm"/></style:style></office:automatic-styles><office:body>{bodies[kind]}</office:body></office:document-content>')
        z.writestr('META-INF/manifest.xml', f'<?xml version="1.0"?><manifest:manifest xmlns:manifest="urn:oasis:names:tc:opendocument:xmlns:manifest:1.0"><manifest:file-entry manifest:full-path="/" manifest:media-type="application/vnd.oasis.opendocument.{mime}"/><manifest:file-entry manifest:full-path="content.xml" manifest:media-type="text/xml"/></manifest:manifest>')
    return path


def make_pdf(path):
    objects = [b'<< /Type /Catalog /Pages 2 0 R >>', b'<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>']
    for number, color in [(1, '1 0 0'), (2, '0 0 1')]:
        stream = f'{color} rg 8 8 24 24 re f 0 0 0 rg BT /F1 22 Tf 8 44 Td (CONVT) Tj ET BT /F1 8 Tf 8 64 Td ({MARKER}_PAGE_{number}) Tj ET'.encode()
        objects += [f'<< /Type /Page /Parent 2 0 R /MediaBox [0 0 144 72] /Resources << /Font << /F1 7 0 R >> >> /Contents {4 if number == 1 else 6} 0 R >>'.encode(), b'<< /Length ' + str(len(stream)).encode() + b' >>\nstream\n' + stream + b'\nendstream']
    objects.append(b'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>')
    data = bytearray(b'%PDF-1.4\n')
    offsets = [0]
    for i, obj in enumerate(objects, 1):
        offsets.append(len(data))
        data.extend(f'{i} 0 obj\n'.encode() + obj + b'\nendobj\n')
    xref = len(data)
    data.extend(f'xref\n0 {len(offsets)}\n0000000000 65535 f \n'.encode())
    for offset in offsets[1:]:
        data.extend(f'{offset:010} 00000 n \n'.encode())
    data.extend(f'trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n'.encode())
    path.write_bytes(data)


def pdf_library():
    library = 'pdfium.dll' if sys.platform == 'win32' else ('libpdfium.dylib' if sys.platform == 'darwin' else 'libpdfium.so')
    candidates = [Path(os.environ.get('CONVT_PDFIUM_DIR', '/nonexistent')) / library,
                  Path(__file__).resolve().parents[1] / 'vendor/pdfium/lib' / library]
    lib = None
    for candidate in candidates:
        if candidate.exists():
            try:
                lib = c.CDLL(str(candidate))
                break
            except OSError:
                pass
    if lib is None:
        name = ctypes.util.find_library('pdfium')
        if not name:
            return None
        try:
            lib = c.CDLL(name)
        except OSError:
            return None
    return lib


def pdf_text(path, render=False):
    lib = pdf_library()
    if lib is None:
        raise RuntimeError("PDFium missing for PDF text validation")
    def fn(name, args, ret):
        f = getattr(lib, name)
        f.argtypes, f.restype = args, ret
        return f
    fn('FPDF_InitLibrary', [], None)()
    doc = fn('FPDF_LoadDocument', [c.c_char_p, c.c_char_p], c.c_void_p)(os.fsencode(path), None)
    if not doc:
        raise RuntimeError('PDFium could not open ' + str(path))
    count = fn('FPDF_GetPageCount', [c.c_void_p], c.c_int)(doc)
    texts = []
    for i in range(count):
        page = fn('FPDF_LoadPage', [c.c_void_p, c.c_int], c.c_void_p)(doc, i)
        text = fn('FPDFText_LoadPage', [c.c_void_p], c.c_void_p)(page)
        size = fn('FPDFText_CountChars', [c.c_void_p], c.c_int)(text)
        buf = (c.c_ushort * (size + 1))()
        fn('FPDFText_GetText', [c.c_void_p, c.c_int, c.c_int, c.POINTER(c.c_ushort)], c.c_int)(text, 0, size, buf)
        texts.append(bytes(buf).decode('utf-16-le').rstrip('\0'))
        if render and i == 0:
            import struct
            width = round(fn('FPDF_GetPageWidthF', [c.c_void_p], c.c_float)(page))
            height = round(fn('FPDF_GetPageHeightF', [c.c_void_p], c.c_float)(page))
            bitmap = fn('FPDFBitmap_Create', [c.c_int, c.c_int, c.c_int], c.c_void_p)(width, height, 1)
            fn('FPDFBitmap_FillRect', [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_int, c.c_ulong], None)(bitmap, 0, 0, width, height, 0xffffffff)
            # Reverse-byte-order plus annotations, matching the documented render defaults.
            fn('FPDF_RenderPageBitmap', [c.c_void_p, c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_int, c.c_int, c.c_int], None)(bitmap, page, 0, 0, width, height, 0, 0x11)
            stride = fn('FPDFBitmap_GetStride', [c.c_void_p], c.c_int)(bitmap)
            data = fn('FPDFBitmap_GetBuffer', [c.c_void_p], c.c_void_p)(bitmap)
            pixels = b''.join(c.string_at(data + y * stride, width * 4) for y in range(height))
            sys.stdout.buffer.write(struct.pack('<II', width, height) + pixels)
            fn('FPDFBitmap_Destroy', [c.c_void_p], None)(bitmap)
        fn('FPDFText_ClosePage', [c.c_void_p], None)(text)
        fn('FPDF_ClosePage', [c.c_void_p], None)(page)
    fn('FPDF_CloseDocument', [c.c_void_p], None)(doc)
    fn('FPDF_DestroyLibrary', [], None)()
    return {'pages': count, 'text': '\n'.join(texts)}


def ffmpeg_heic(dest):
    """Encode color and alpha with FFmpeg, then write their standard HEIF items."""
    import struct
    u16 = lambda n: struct.pack('>H', n)
    u32 = lambda n: struct.pack('>I', n)
    box = lambda tag, data: u32(8 + len(data)) + tag + data
    tool = os.environ.get('CONVT_FFMPEG') or shutil.which('ffmpeg')
    if not tool:
        sys.exit(77)
    def encode(alpha):
        movie = dest / ('alpha.mov' if alpha else 'hevc.mov')
        args = [tool, '-v', 'error', '-y', '-f', 'rawvideo', '-pixel_format', 'rgba',
                '-video_size', '64x48', '-i', str(dest / 'pattern.rgba'), '-frames:v', '1']
        if alpha:
            args += ['-vf', 'alphaextract,format=gray', '-pix_fmt', 'gray']
        else:
            args += ['-pix_fmt', 'yuv420p']
        args += ['-c:v', 'libx265', '-x265-params', 'pools=1:frame-threads=1:log-level=error',
                 '-crf', '10', '-tag:v', 'hvc1', str(movie)]
        result = subprocess.run(args, capture_output=True, timeout=30)
        if result.returncode:
            print(result.stderr.decode(), file=sys.stderr)
            sys.exit(77)
        data = movie.read_bytes()
        pos = data.index(b'hvcC') - 4
        hvcc = data[pos:pos + struct.unpack('>I', data[pos:pos + 4])[0]]
        pos = data.index(b'mdat') - 4
        sample = data[pos + 8:pos + struct.unpack('>I', data[pos:pos + 4])[0]]
        return hvcc, sample
    color_hvcc, color_sample = encode(False)
    alpha_hvcc, alpha_sample = encode(True)
    ftyp = box(b'ftyp', b'heic' + u32(0) + b'heicmif1')
    hdlr = box(b'hdlr', bytes(8) + b'pict' + bytes(12) + b'convt\0')
    pitm = box(b'pitm', bytes(4) + u16(1))
    def infe(item):
        return box(b'infe', b'\x02' + bytes(3) + u16(item) + u16(0) + b'hvc1' + b'pattern\0')
    iinf = box(b'iinf', bytes(4) + u16(2) + infe(1) + infe(2))
    ispe = box(b'ispe', bytes(4) + u32(64) + u32(48))
    pixi = box(b'pixi', bytes(4) + b'\x03\x08\x08\x08')
    alpha_pixi = box(b'pixi', bytes(4) + b'\x01\x08')
    auxc = box(b'auxC', bytes(4) + b'urn:mpeg:hevc:2015:auxid:1\0')
    ipco = box(b'ipco', color_hvcc + ispe + pixi + alpha_hvcc + alpha_pixi + auxc)
    ipma = box(b'ipma', bytes(4) + u32(2) + u16(1) + b'\x03\x81\x82\x83' + u16(2) + b'\x04\x84\x82\x85\x86')
    iprp = box(b'iprp', ipco + ipma)
    iref = box(b'iref', bytes(4) + box(b'auxl', u16(2) + u16(1) + u16(1)))
    def meta(offset):
        def location(item, pos, length):
            return u16(item) + u16(0) + u32(0) + u16(1) + u32(pos) + u32(length)
        iloc = box(b'iloc', bytes(4) + b'\x44\x40' + u16(2) + location(1, offset, len(color_sample)) + location(2, offset + len(color_sample), len(alpha_sample)))
        return box(b'meta', bytes(4) + hdlr + pitm + iloc + iinf + iprp + iref)
    metadata = meta(len(ftyp) + len(meta(0)) + 8)
    (dest / 'sample.heic').write_bytes(ftyp + metadata + box(b'mdat', color_sample + alpha_sample))


def heic(dest, compression=1):
    name = (str(Path(os.environ['CONVT_LIBHEIF_DIR']) / 'libheif.so.1') if os.environ.get('CONVT_LIBHEIF_DIR') else ctypes.util.find_library('heif'))
    if not name:
        sys.exit(77)
    lib = c.CDLL(name)
    class Error(c.Structure):
        _fields_ = [('code', c.c_int), ('subcode', c.c_int), ('message', c.c_char_p)]
    def fn(name, args, ret):
        f = getattr(lib, name)
        f.argtypes, f.restype = args, ret
        return f
    def check(error):
        if error.code:
            raise RuntimeError(error.message.decode())
    alloc = fn('heif_context_alloc', [], c.c_void_p)
    ctx = alloc()
    encoder, img, handle = c.c_void_p(), c.c_void_p(), c.c_void_p()
    try:
        error = fn('heif_context_get_encoder_for_format', [c.c_void_p, c.c_int, c.POINTER(c.c_void_p)], Error)(ctx, compression, c.byref(encoder))
        if error.code:
            if compression != 1:
                sys.exit(77)
            ffmpeg_heic(dest)
            print('alpha', end='')
            return
        check(fn('heif_encoder_set_lossy_quality', [c.c_void_p, c.c_int], Error)(encoder, 95))
        # x265 otherwise creates a pool for every CPU on this small fixture.
        fn('heif_encoder_set_parameter_string', [c.c_void_p, c.c_char_p, c.c_char_p], Error)(encoder, b'x265:pools', b'1')
        check(fn('heif_image_create', [c.c_int, c.c_int, c.c_int, c.c_int, c.POINTER(c.c_void_p)], Error)(64, 48, 1, 11, c.byref(img)))
        check(fn('heif_image_add_plane', [c.c_void_p, c.c_int, c.c_int, c.c_int, c.c_int], Error)(img, 10, 64, 48, 8))
        stride = c.c_int()
        plane = fn('heif_image_get_plane', [c.c_void_p, c.c_int, c.POINTER(c.c_int)], c.c_void_p)(img, 10, c.byref(stride))
        raw = (dest / 'pattern.rgba').read_bytes()
        for y in range(48):
            c.memmove(plane + y * stride.value, raw[y * 256:(y + 1) * 256], 256)
        check(fn('heif_context_encode_image', [c.c_void_p, c.c_void_p, c.c_void_p, c.c_void_p, c.POINTER(c.c_void_p)], Error)(ctx, img, encoder, None, c.byref(handle)))
        check(fn('heif_context_write_to_file', [c.c_void_p, c.c_char_p], Error)(ctx, os.fsencode(dest / ('sample.heic' if compression == 1 else 'sample.avif'))))
        print('alpha', end='')
    finally:
        if handle: fn('heif_image_handle_release', [c.c_void_p], None)(handle)
        if img: fn('heif_image_release', [c.c_void_p], None)(img)
        if encoder: fn('heif_encoder_release', [c.c_void_p], None)(encoder)
        fn('heif_context_free', [c.c_void_p], None)(ctx)


def capabilities():
    heif = False
    hevc = False
    try:
        name = (str(Path(os.environ['CONVT_LIBHEIF_DIR']) / 'libheif.so.1') if os.environ.get('CONVT_LIBHEIF_DIR') else ctypes.util.find_library('heif'))
        if name:
            lib = c.CDLL(name)
            if hasattr(lib, 'heif_init'):
                class Error(c.Structure):
                    _fields_ = [('code', c.c_int), ('subcode', c.c_int), ('message', c.c_char_p)]
                lib.heif_init.argtypes = [c.c_void_p]
                lib.heif_init.restype = Error
                if lib.heif_init(None).code:
                    return {'pdfium': pdf_library() is not None, 'libheif': False, 'hevc': False}
            heif = bool(lib.heif_have_decoder_for_format(4))
            hevc = bool(lib.heif_have_decoder_for_format(1))
    except (OSError, AttributeError):
        pass
    return {'pdfium': pdf_library() is not None, 'libheif': heif, 'hevc': hevc}


def heif_pixels(path):
    import struct
    lib = c.CDLL((str(Path(os.environ['CONVT_LIBHEIF_DIR']) / 'libheif.so.1') if os.environ.get('CONVT_LIBHEIF_DIR') else ctypes.util.find_library('heif')))
    class Error(c.Structure):
        _fields_ = [('code', c.c_int), ('subcode', c.c_int), ('message', c.c_char_p)]
    def fn(name, args, ret):
        f = getattr(lib, name)
        f.argtypes, f.restype = args, ret
        return f
    def check(err):
        if err.code:
            raise RuntimeError(err.message.decode())
    if hasattr(lib, 'heif_init'):
        check(fn('heif_init', [c.c_void_p], Error)(None))
    ctx = fn('heif_context_alloc', [], c.c_void_p)()
    handle, img = c.c_void_p(), c.c_void_p()
    try:
        check(fn('heif_context_read_from_file', [c.c_void_p, c.c_char_p, c.c_void_p], Error)(ctx, os.fsencode(path), None))
        check(fn('heif_context_get_primary_image_handle', [c.c_void_p, c.POINTER(c.c_void_p)], Error)(ctx, c.byref(handle)))
        check(fn('heif_decode_image', [c.c_void_p, c.POINTER(c.c_void_p), c.c_int, c.c_int, c.c_void_p], Error)(handle, c.byref(img), 1, 11, None))
        width = fn('heif_image_get_width', [c.c_void_p, c.c_int], c.c_int)(img, 10)
        height = fn('heif_image_get_height', [c.c_void_p, c.c_int], c.c_int)(img, 10)
        stride = c.c_int()
        plane = fn('heif_image_get_plane_readonly', [c.c_void_p, c.c_int, c.POINTER(c.c_int)], c.c_void_p)(img, 10, c.byref(stride))
        pixels = b''.join(c.string_at(plane + y * stride.value, width * 4) for y in range(height))
        sys.stdout.buffer.write(struct.pack('<II', width, height) + pixels)
    finally:
        if img: fn('heif_image_release', [c.c_void_p], None)(img)
        if handle: fn('heif_image_handle_release', [c.c_void_p], None)(handle)
        fn('heif_context_free', [c.c_void_p], None)(ctx)


def office_check(path, kind):
    data = path.read_bytes()
    ext = path.suffix[1:]
    if ext in ['docx', 'pptx', 'xlsx', 'odt', 'odp', 'ods']:
        with zipfile.ZipFile(path) as z:
            required = {'docx': 'word/document.xml', 'pptx': 'ppt/presentation.xml', 'xlsx': 'xl/workbook.xml', 'odt': 'content.xml', 'odp': 'content.xml', 'ods': 'content.xml'}[ext]
            assert required in z.namelist(), 'Wrong Office container'
            if ext in ['odt', 'odp', 'ods']:
                assert z.read('mimetype').decode().endswith({'odt': 'text', 'odp': 'presentation', 'ods': 'spreadsheet'}[ext])
    elif ext in ['doc', 'ppt', 'xls']:
        assert data.startswith(bytes.fromhex('d0cf11e0a1b11ae1')), 'Wrong OLE magic'
        stream = {'doc': 'WordDocument', 'ppt': 'PowerPoint Document', 'xls': 'Workbook'}[ext]
        assert stream.encode('utf-16-le') in data, 'Wrong OLE document type'
    elif ext == 'rtf':
        assert data.startswith(b'{\\rtf'), 'Wrong RTF magic'
    elif ext == 'html':
        assert b'<html' in data.lower(), 'Wrong HTML magic'
    with tempfile.TemporaryDirectory(prefix='convt-office-check-') as tmp:
        if kind == 'Spreadsheet':
            import csv
            out = path if ext == 'csv' else office_convert(path, 'csv:Text - txt - csv (StarCalc):44,34,76', Path(tmp))
            rows = list(csv.reader(out.read_text(encoding='utf-8-sig').splitlines()))
            assert rows[0][0] == MARKER and float(rows[0][1]) == 42, f'Wrong A1/B1: {rows}'
            assert rows[1][0] == 'KnownCell' and float(rows[1][1]) == 3.5, f'Wrong A2/B2: {rows}'
            text = out.read_text()
        elif kind == 'Presentation':
            out = office_convert(path, 'pdf', Path(tmp))
            info = pdf_text(out)
            assert info['pages'] == 1, info
            text = info['text']
        else:
            out = path if ext == 'txt' else office_convert(path, 'txt:Text (encoded):UTF8', Path(tmp))
            text = out.read_text(encoding='utf-8-sig')
        assert MARKER in text, 'Marker lost: ' + repr(text)
        if kind == 'Document':
            assert 'Known document text 12345' in text, 'Document text lost'


if __name__ == '__main__':
    mode, path, *rest = sys.argv[1:]
    path = Path(path).resolve()
    if mode == 'capabilities': print(json.dumps(capabilities()))
    elif mode == 'pdf': make_pdf(path)
    elif mode == 'pdf-check': print(json.dumps(pdf_text(path)))
    elif mode == 'pdf-pixels': pdf_text(path, True)
    elif mode == 'heif-pixels': heif_pixels(path)
    elif mode == 'heic': heic(path)
    elif mode == 'avif': heic(path, 4)
    elif mode == 'office-fixture':
        kind, target = rest
        src = office_fixture(path, kind)
        if target != kind:
            with tempfile.TemporaryDirectory(prefix='convt-office-fixture-') as tmp:
                out = office_convert(src, target, Path(tmp))
                shutil.copyfile(out, path / ('sample.' + target))
        if pdf_library() is not None:
            reference = office_convert(path / ('sample.' + target), 'pdf', path / 'reference')
            info = pdf_text(reference)
            assert info['pages'] == 1 and MARKER in info['text'], f'Invalid fixture reference: {info}'
    elif mode == 'office-check': office_check(path, rest[0])
    else: raise ValueError(mode)
