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
import os.path

class ImportTests(unittest.TestCase):
    """Import every .gmf file in the RA2 tree"""

    def importTest(self, filename):
        from PyRA2.GMIModel import loadScene
        loadScene(open(filename, 'rb'))

def createTests(arg, dirname, names):
    for name in names:
        base, ext = os.path.splitext(name)
        if ext == '.gmf':
            if base.endswith('_txt'): continue
            testName = 'testImport_%s' % base
            assert not hasattr(ImportTests, testName), (
                'Duplicate file %s' % name)
            sceneFile = os.path.join(dirname, name)
            setattr(ImportTests, testName, 
                    lambda self, s=sceneFile: self.importTest(s))

base = r'C:\Program Files\Infogrames\Robot Arena 2'
os.path.walk(base, createTests, None)

if __name__ == '__main__':
    unittest.main()
