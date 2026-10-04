# Third-party notices

Mariam Flow is distributed under the PolyForm Noncommercial License 1.0.0
(see [LICENSE.md](LICENSE.md)). It incorporates third-party components whose
own licenses require their copyright notices to travel with the software.
This file carries those notices.

It covers what is **distributed**: code and assets that end up inside the
appliance binary or the firmware image. Tools used only to build the project
— compilers, formatters, test runners — are not distributed and are not
listed here.

Components are named, not versioned: the exact versions of any build are
recorded in `Cargo.lock` and `dashboard/pnpm-lock.yaml`, which are
authoritative. This file changes when a component is added or removed, not
when one is updated.

---

## Dashboard assets, compiled into the appliance binary

### Inter — SIL Open Font License 1.1

Bundled as `@fontsource-variable/inter`.

```
Copyright 2016 The Inter Project Authors (https://github.com/rsms/inter)
Inter-Italic[opsz,wght].ttf: Copyright 2016 The Inter Project Authors
(https://github.com/rsms/inter)

This Font Software is licensed under the SIL Open Font License, Version 1.1.
This license is copied below, and is also available with a FAQ at:
http://scripts.sil.org/OFL


-----------------------------------------------------------
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007
-----------------------------------------------------------

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded,
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.

DEFINITIONS
"Font Software" refers to the set of files released by the Copyright
Holder(s) under this license and clearly marked as such. This may
include source files, build scripts and documentation.

"Reserved Font Name" refers to any names specified as such after the
copyright statement(s).

"Original Version" refers to the collection of Font Software components as
distributed by the Copyright Holder(s).

"Modified Version" refers to any derivative made by adding to, deleting,
or substituting -- in part or in whole -- any of the components of the
Original Version, by changing formats or by porting the Font Software to a
new environment.

"Author" refers to any designer, engineer, programmer, technical
writer or other person who contributed to the Font Software.

PERMISSION & CONDITIONS
Permission is hereby granted, free of charge, to any person obtaining
a copy of the Font Software, to use, study, copy, merge, embed, modify,
redistribute, and sell modified and unmodified copies of the Font
Software, subject to the following conditions:

1) Neither the Font Software nor any of its individual components,
in Original or Modified Versions, may be sold by itself.

2) Original or Modified Versions of the Font Software may be bundled,
redistributed and/or sold with any software, provided that each copy
contains the above copyright notice and this license. These can be
included either as stand-alone text files, human-readable headers or
in the appropriate machine-readable metadata fields within text or
binary files as long as those fields can be easily viewed by the user.

3) No Modified Version of the Font Software may use the Reserved Font
Name(s) unless explicit written permission is granted by the corresponding
Copyright Holder. This restriction only applies to the primary font name as
presented to the users.

4) The name(s) of the Copyright Holder(s) or the Author(s) of the Font
Software shall not be used to promote, endorse or advertise any
Modified Version, except to acknowledge the contribution(s) of the
Copyright Holder(s) and the Author(s) or with their explicit written
permission.

5) The Font Software, modified or unmodified, in part or in whole,
must be distributed entirely under this license, and must not be
distributed under any other license. The requirement for fonts to
remain under this license does not apply to any document created
using the Font Software.

TERMINATION
This license becomes null and void if any of the above conditions are
not met.

DISCLAIMER
THE FONT SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO ANY WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT
OF COPYRIGHT, PATENT, TRADEMARK, OR OTHER RIGHT. IN NO EVENT SHALL THE
COPYRIGHT HOLDER BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
INCLUDING ANY GENERAL, SPECIAL, INDIRECT, INCIDENTAL, OR CONSEQUENTIAL
DAMAGES, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF THE USE OR INABILITY TO USE THE FONT SOFTWARE OR FROM
OTHER DEALINGS IN THE FONT SOFTWARE.
```

### Lucide icons — ISC License

Bundled as `@lucide/svelte`.

```
ISC License

Copyright (c) 2026 Lucide Icons and Contributors

Permission to use, copy, modify, and/or distribute this software for any
purpose with or without fee is hereby granted, provided that the above
copyright notice and this permission notice appear in all copies.

THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

---

The following Lucide icons are derived from the Feather project:

airplay, alert-circle, alert-octagon, alert-triangle, aperture, arrow-down-
circle, arrow-down-left, arrow-down-right, arrow-down, arrow-left-circle,
arrow-left, arrow-right-circle, arrow-right, arrow-up-circle, arrow-up-left,
arrow-up-right, arrow-up, at-sign, calendar, cast, check, chevron-down,
chevron-left, chevron-right, chevron-up, chevrons-down, chevrons-left,
chevrons-right, chevrons-up, circle, clipboard, clock, code, columns,
command, compass, corner-down-left, corner-down-right, corner-left-down,
corner-left-up, corner-right-down, corner-right-up, corner-up-left, corner-
up-right, crosshair, database, divide-circle, divide-square, dollar-sign,
download, external-link, feather, frown, hash, headphones, help-circle,
info, italic, key, layout, life-buoy, link-2, link, loader, lock, log-in,
log-out, maximize, meh, minimize, minimize-2, minus-circle, minus-square,
minus, monitor, moon, more-horizontal, more-vertical, move, music,
navigation-2, navigation, octagon, pause-circle, percent, plus-circle, plus-
square, plus, power, radio, rss, search, server, share, shopping-bag,
sidebar, smartphone, smile, square, table-2, tablet, target, terminal,
trash-2, trash, triangle, tv, type, upload, x-circle, x-octagon, x-square,
x, zoom-in, zoom-out

The MIT License (MIT) (for the icons listed above)

Copyright (c) 2013-present Cole Bemis

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to
deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE
SOFTWARE.
```

### Svelte — MIT License

The Svelte runtime is compiled into the dashboard bundle.

```
Copyright (c) 2016-2025 [Svelte
Contributors](https://github.com/sveltejs/svelte/graphs/contributors)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to
deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
IN THE SOFTWARE.
```

### SvelteKit — MIT License

Parts of the SvelteKit client runtime are compiled into the dashboard
bundle.

```
Copyright (c) 2020 [these
people](https://github.com/sveltejs/kit/graphs/contributors)

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to
deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or
sell copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS
IN THE SOFTWARE.
```

---

## Rust dependencies, compiled into the appliance binary

The daemon links a substantial tree of Rust crates, recorded exactly in
`Cargo.lock`. That file is the authoritative list, and it is a *superset* of
what ships: it resolves the dependency graph for every platform and feature
combination, so it also names crates that only build for Windows, Redox or
WebAssembly, and procedural macros that run during compilation without
entering the binary at all.

The project takes only permissively licensed dependencies. Most are
dual-licensed under MIT and Apache-2.0. Where a crate carries something else
— a BSD or Zlib variant, the Unicode license covering character tables, or
the Mozilla Public License 2.0, whose obligations are per-file and attach
only to modifications of that crate's own source — the terms remain
permissive and impose no condition on the work as a whole. No dependency
places a copyleft obligation on the appliance binary.

SQLite itself, vendored through `rusqlite`'s bundled build, is in the public
domain and imposes no condition.

> **Completeness.** This section states the licensing posture rather than
> reproducing several hundred notices, which could not be kept accurate by
> hand. Emitting the exhaustive per-crate listing mechanically — resolved for
> the target platform, so that it reflects what actually ships rather than
> the whole of `Cargo.lock` — is the intended next step; `cargo about` or an
> equivalent is the tool for it.

---

## Firmware

The node firmware derives from `espressif/esp-csi` and retains the Apache
License 2.0 notices inherited from it, in `firmware/`. See
[LICENSE.md](LICENSE.md).
