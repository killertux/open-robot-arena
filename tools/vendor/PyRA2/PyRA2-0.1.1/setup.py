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

"""Setup support

Installation (with dependecy checking) and distribution building using 
distutils.py

"""
# Dependencies
import sys

if sys.version < '2.2':
    print "Error: Python version too old. You need at least Python 2.2 to " \
          "use this package."
    print "(You are running version %s)" % sys.version
    sys.exit(0)

try:
    import Numeric
except ImportError:
    print "Error: Numeric package not found. This package requires that " \
          "Numeric is installed."
    sys.exit(0)

try:
    import Image
except ImportError:
    print "Error: Python Image Library (PIL) package not found. This package " \
          "requires that PIL is installed."
    sys.exit(0)

# Install
from distutils.core import setup

sys.path.insert(0, 'lib/') # Add library subdir to the path.
from PyRA2 import __version__, __doc__

# Patch distutils if DistributionMetaData doesn't know about more recent 
# metadata (Py 2.3 and up)
from distutils.dist import DistributionMetadata
if not hasattr(DistributionMetadata, 'classifiers'):
    DistributionMetadata.classifiers = None
if not hasattr(DistributionMetadata, 'download_url'):
    DistributionMetadata.download_url = None

scriptnames = (
    'gmi2vrml',
    'bot2vrml',
    'convertAllBots',
    'convertAllComponents',
)

doclines = __doc__.splitlines()

classifiers = """\
Development Status :: 2 - Pre-Alpha
Environment :: Console
Environment :: Other Environment
Environment :: Web Environment
Environment :: Win32 (MS Windows)
Intended Audience :: Developers
License :: OSI Approved :: GNU Library or Lesser General Public License (LGPL)
Natural Language :: English
Operating System :: OS Independent
Programming Language :: Python
Topic :: Internet :: WWW/HTTP :: Dynamic Content
Topic :: Games/Entertainment
Topic :: Multimedia :: Graphics
"""

setup(name="PyRA2",
      version=__version__,
      description=doclines[0],
      long_description='\n'.join(doclines[2:]),
      author="Martijn Pieters",
      author_email="pyra2@zopatista.com",
      url="http://sf.net/projects/pyra2",
      download_url="http://sf.net/project/showfiles.php?group_id=81504",
      license="http://www.gnu.org/licenses/lgpl.txt",
      classifiers=filter(None, classifiers.splitlines()),
      platforms=['any'],

      package_dir={'': 'lib'},
      packages=['PyRA2', 'PyRA2.GMIModel', 'PyRA2.RA2Bot'],
      scripts=['scripts/%s.py' % s for s in scriptnames],
      script_name='setup.py'
)
