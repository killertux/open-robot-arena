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

import unittest
from cStringIO import StringIO

class AsciiImporterTests(unittest.TestCase):
    """Test the field parsing capabilities"""

    def _createOne(self, data):
        from PyRA2.RA2Bot.Import import AsciiImporter
        return AsciiImporter(StringIO(data))

    def testLiteralAndString(self):
        importer = self._createOne('foo: bar baz\n')
        self.assertEqual(importer.readData("'foo:' s"), 'bar baz')

    def testSimpleFields(self):
        importer = self._createOne('1 true 0.5e-1 false foo bar\n')
        self.assertEqual(importer.readData("i b f b s"),
                         (1, 1, 0.05, 0, 'foo bar'))

    def testSingleFields(self):
        importer = self._createOne('1\ntrue\n0.5e-1\nfalse\nfoo bar\n')
        self.assertEqual(importer.readInt(), 1)
        self.assertEqual(importer.readBoolean(), 1)
        self.assertEqual(importer.readFloat(), 0.05)
        self.assertEqual(importer.readBoolean(), 0)
        self.assertEqual(importer.readString(), 'foo bar')
        

if __name__ == '__main__':
    unittest.main()
