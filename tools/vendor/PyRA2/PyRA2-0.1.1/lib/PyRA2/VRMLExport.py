# PyRA2: Python support for Robot Arena 2 file formats.
# Copyright (C) 2003  Martijn Pieters <pyra2@zopatista.com>
#
# This library is free software; you can redistribute it and/or
# modify it under the terms of the GNU Lesser General Public
# License as published by the Free Software Foundation; either
# version 2.1 of the License, or (at your option) any later version.
#
# This library is distributed in the hope that it will be useful,
# but WITHOUT ANY WARRANTY; without even the implied warranty of
# MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU
# Lesser General Public License for more details.
#
# You should have received a copy of the GNU Lesser General Public
# License along with this library; if not, write to the Free Software
# Foundation, Inc., 59 Temple Place, Suite 330, Boston, MA  02111-1307  USA

"""Base support for VRML Exporters."""

from types import StringType

class AsciiExporter:
    _fd = None
    _indent = ''
    
    def __init__(self, fd):
        self._fd = fd

    def indent(self):
        self._indent += '  '

    def dedent(self):
        self._indent = self._indent[:-2]

    def open(self, label, delimiter='{'):
        if label: label += ' '
        self.writeline('%s%s' % (label, delimiter))
        self.indent()

    def close(self, delimiter='}'):
        self.dedent()
        self.writeline(delimiter)

    def write(self, text):
        self._fd.write(text.encode('utf8'))

    def writeline(self, chars):
        chars = chars and '%s%s' % (self._indent, chars.strip())
        self.write('%s\n' % chars)

    def writelines(self, text):
        if isinstance(text, StringType):
            text = text.splitlines()
        for line in text:
            self.writeline(line)   
