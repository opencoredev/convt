// SPDX-License-Identifier: AGPL-3.0-only
// The distro liblangtag API accepts a private data directory. Initialize it
// before LibreOffice uses language tags; no system files need to be installed.
#include <dlfcn.h>
#include <stdlib.h>
__attribute__((constructor)) static void convt_langtag_init(void) {
    const char *directory = getenv("CONVT_LANGTAG_DIR");
    if (!directory) return;
    void *library = dlopen("liblangtag.so.1", RTLD_NOW | RTLD_GLOBAL);
    if (!library) return;
    void (*set_directory)(const char *) = dlsym(library, "lt_db_set_datadir");
    if (set_directory) set_directory(directory);
}
