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

"""Transformation matrices with expected decompositions.

Matrices were created to test scale, scale orientation and determinant sign,
and tested against the original Graphic Gems C code.

"""

from math import pi, sqrt

test_matrices = (
    # Manually generated matrices

    # Matrix 0, identity
    (
        (
            (1.0, 0.0, 0.0),
            (0.0, 1.0, 0.0),
            (0.0, 0.0, 1.0),
            (0.0, 0.0, 0.0)
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (0.0, 0.0, 1.0, 0.0),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 1, translation
    (
        (
            ( 1.0,       0.0,      0.0),
            ( 0.0,       1.0,      0.0),
            ( 0.0,       0.0,      1.0),
            (-9.382856, 11.261452,-3.519421)
        ), {
            'translation':      (-9.382856, 11.261452, -3.519421),
            'rotation':         (0.0, 0.0, 1.0, 0.0),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 2, rotation by pi around X
    (
        (
            (1.0, 0.0,             0.0),
            (0.0,-1.0,             1.22460635e-016),
            (0.0,-1.22460635e-016,-1.0),
            (0.0, 0.0,             0.0),
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (1.0, 0.0, 0.0, pi),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),
    
    # Matrix 3, rotation by pi around Y
    (
        (
            (-1.0,             0.0,-1.22460635e-016),
            ( 0.0,             1.0, 0.0),
            ( 1.22460635e-016, 0.0,-1.0),
            ( 0.0,             0.0, 0.0),
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (0.0, 1.0, 0.0, pi),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 4, rotation by pi around Z
    (
        (
            (-1.0,             1.22460635e-016, 0.0),
            (-1.22460635e-016,-1.0,             0.0),
            ( 0.0,             0.0,             1.0),
            ( 0.0,             0.0,             0.0),
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (0.0, 0.0, 1.0, pi),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 5, rotation by pi around X and Y
    (
        (
            (-2.22044605e-016, 1.0,            -8.65927457e-017),
            ( 1.0,            -2.22044605e-016, 8.65927457e-017),
            ( 8.65927457e-017,-8.65927457e-017,-1.0),
            ( 0.0,             0.0,             0.0)
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (1/sqrt(2), 1/sqrt(2), 0.0, pi),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 6, rotation by pi around Y and Z
    (
        (
            (-1.0            , 8.65927457e-017,-8.65927457e-017),
            (-8.65927457e-017,-2.22044605e-016, 1.0),
            ( 8.65927457e-017, 1.0            ,-2.22044605e-016),
            ( 0.0,             0.0,             0.0)
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (0.0, 1/sqrt(2), 1/sqrt(2), pi),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 7, rotation by pi around Z and X
    (
        (
            (-2.22044605e-016, 8.65927457e-017, 1.0),
            (-8.65927457e-017,-1.0            , 8.65927457e-017),
            ( 1.0            ,-8.65927457e-017,-2.22044605e-016),
            ( 0.0,             0.0,             0.0)
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (1/sqrt(2), 0.0, 1/sqrt(2), pi),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 8, rotation by pi around X, Y and Z
    (
        (
            (-1/3.0, 2/3.0, 2/3.0),
            ( 2/3.0,-1/3.0, 2/3.0),
            ( 2/3.0, 2/3.0,-1/3.0),
            (   0.0,   0.0,   0.0)
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (1/sqrt(3), 1/sqrt(3), 1/sqrt(3), pi),
            'scale':            (1.0, 1.0, 1.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 9, Unrotated scaling
    (
        (
            (5.0, 0.0, 0.0),
            (0.0, 6.0, 0.0),
            (0.0, 0.0, 7.0),
            (0.0, 0.0, 0.0)
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (0.0, 0.0, 1.0, 0.0),
            'scale':            (5.0, 6.0, 7.0),
            'scaleOrientation': (0.0, 0.0, 1.0, 0.0),
            'determinantSign':  1,
        }
    ),

    # Matrix 10, Rotated scaling (by Pi around X, Y and Z)
    (
        (
            ( 0.15978835978836,-0.01798941798942,-0.00211640211640),
            (-0.01798941798942, 0.17089947089947, 0.02010582010582),
            (-0.00211640211640, 0.02010582010582, 0.17883597883598),
            ( 0.0,              0.0,              0.0)
        ), {
            'translation':      (0.0, 0.0, 0.0),
            'rotation':         (0.0, 0.0, 1.0, 0.0),
            'scale':            (1/7.0, 1/5.0, 1/6.0),
            'scaleOrientation': (-1/sqrt(3), -1/sqrt(3), -1/sqrt(3), pi/3),
            'determinantSign':  1,
        }
    ),

    # Matrix 11, translation, rotation and scaling with semi-random values
    (
        (
            ( 46112.557899857136,-36108.645229422094, 29401.961624351366),
            (-40183.657251738317, 31455.033206490196,-25627.373271476972),
            ( 86106.618720093829,-67419.910042812364, 54919.05840290981),
            (-272807.68087811605, 213631.88182140369,-174026.18816125923),
        ), {
            'translation':      (-272807.68087811605, 213631.88182140369,
                                 -174026.18816125923),
            'rotation':         (0.45517725366775, 0.86168434323435,
                                 0.22430729004712, 3.6191899855121),
            'scale':            (9.3351097420203, 4.7170089227476,
                                 150104.90402625),
            'scaleOrientation': (0.64432594469177, 0.43351898349902,
                                 0.63000426025785, 0.80418741432538),
            'determinantSign':  -1,
        }
    ),

    # Matrix 12, same effect as 11, only normalized.
    (
        (
            (-0.047565759511647682, 0.11728062274267828, 0.080192943222394925),
            ( 0.024533737235546908, 0.11419012778685148, 0.040150962018177312),
            ( 0.10469569659501879,-0.043699760927907678,-0.076461051241777878),
            ( 2.2999999983258022, -4.5000000024474502,  -6.5000000002510205),
        ), {
            'translation':      (2.2999999983258022, -4.5000000024474502,
                                 -6.5000000002510205),
            'rotation':         (0.45517725366774603, 0.86168434323434584,
                                 0.2243072900471195, 2.6639953216674845),
            'scale':            (0.21199875098339302, 6.6620075239386533e-006,
                                 0.10712246857674301),
            'scaleOrientation': (-0.65305837764720565, 0.35454153904889435,
                                  0.66918984785607827, 1.0628776450914139),
            'determinantSign':  1,
        }
    ),
)
