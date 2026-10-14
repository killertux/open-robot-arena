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

"""Unit tests for the matrix components module"""

import unittest
import os.path
import sys

class MatrixComponentsTests(unittest.TestCase):
    """Test matrix decomponsition against predermined results."""

    def assertAboutEqual(self, t1, t2, *args):
        """Test for float equality up to 12 decimals"""
        format = '%.14g ' * len(t1)
        self.assertEqual(format % t1, format % t2, *args)

    def matrixComponentTest(self, matrix, expected):
        from PyRA2.MatrixComponents import TransformMatrix
        tm = TransformMatrix(matrix)
        self.assertAboutEqual(tm.getTranslation(), expected['translation'])
        self.assertAboutEqual(tm.getRotation(), expected['rotation'])
        self.assertAboutEqual(tm.getScale(), expected['scale'])
        self.assertAboutEqual(tm.getScaleOrientation(),
                              expected['scaleOrientation'])
        self.assertEqual(tm.getDeterminantSign(), expected['determinantSign'])

# Read matrix data and expected results
execfile(os.path.join(os.path.dirname(sys.argv[0]), 'matrix_data.py'))
count = 0
for matrix, expected in test_matrices:
    setattr(MatrixComponentsTests, 'testMatrix%02d' % count,
            lambda self, m=matrix, e=expected: self.matrixComponentTest(m, e))
    count += 1

if __name__ == '__main__':
    unittest.main()
