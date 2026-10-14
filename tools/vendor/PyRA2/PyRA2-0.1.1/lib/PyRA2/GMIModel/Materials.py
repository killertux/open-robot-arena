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

"""Model materials."""

from NodeList import NodeList

#
# Materials
#
materialClasses = {}
class MaterialList(NodeList):
    """All materials in a scene grouped together"""

    def _createMaterial(self, klass):
        """Add an empty Material node by class to the list and return it.
        
        Used by importer.

        """
        self._items += (materialClasses[klass](),)
        return self._items[-1]

#
# Material types
#
SHADING = {
    #?: 'Constant',
    #?: 'Phong',
    #?: 'Metal',
    12: 'Blinn',

    0:  'Other',
}

FALLOFF = {
    0: 'In',
    1: 'Out',
}

XPTYPE = {
    1: 'Filter',
    2: 'Subtractive',
    3: 'Additive',
    
    0: 'Other',
}   

class Material:
    """Simple Material"""
    _class = 'Standard'
    _name = ''
    _ambient = _diffuse = _specular = (0.0, 0.0, 0.0)
    _shine = _shine_str = _trans = _wire = _xp_falloff = _self_illum = 0.0
    _shading = _two_sided = _falloff = _xp_type = 0

    _textures = None

    def __init__(self):
        self._textures = TextureList()

    def getName(self):
        """Material name"""
        return self._name

    def getClass(self):
        """Material class"""
        return self._class

    def getAmbientColor(self):
        """Ambient color"""
        return self._ambient

    def getDiffuseColor(self):
        """Diffuse color"""
        return self._diffuse

    def getSpecularColor(self):
        """Specular color"""
        return self._specular

    def getShine(self):
        """Shine (reflection) intensity"""
        return self._shine

    def getShineStrength(self):
        """Shininess (highlight intensity)"""
        return self._shine_str

    def getTransparency(self):
        """Transparency"""
        return self._trans

    def getWireSize(self):
        """Wireframe wire size"""
        return self._wire

    def getShading(self):
        """Shading type"""
        return SHADING[self._shading]

    def getTransparencyFalloff(self):
        """Transparency falloff"""
        return self._xp_falloff

    def getSelfIllumination(self):
        """Self Illumination"""
        return self._self_illum

    def getTwoSided(self):
        """Wether or not this is to be seen two-sided."""
        return self._two_sided

    def getFalloff(self):
        """Fall off (in or out)"""
        return FALLOFF[self._falloff]

    def getTransparencyType(self):
        """Transparency Type"""
        return XPTYPE[self._xp_type]

    def getTextureCount(self):
        return len(self._textures)

    def getTexture(self, idx):
        return self._textures[idx]

    def getTextures(self):
        return self._textures

materialClasses[Material._class] = Material


class MultiMaterial(Material):
    """A multi-material node

    Provides materials for every face group of a geomotry node.

    """
    _class = 'Multi/Sub-Object'
    _sub_materials = None

    def __init__(self):
        self._sub_materials = MaterialList()

    def getSubMaterialCount(self):
        return len(self._sub_materials)

    def getSubMaterial(self, idx):
        return self._sub_materials[idx]

    def getSubMaterials(self):
        return self._sub_materials

materialClasses[MultiMaterial._class] = MultiMaterial

#
# Textures
#
textureClasses = {}
class TextureList(NodeList):
    """All textures for a material"""

    def _createTexture(self, klass):
        """Add an empty Material node by class to the list and return it
        
        Used by the importer.

        """
        self._items += (textureClasses[klass](),)
        return self._items[-1]
    
#
# Texture types
#
class BaseTexture:
    """A material texture"""
    _class = 'Base'
    _name = ''
    _amount = 0.0

    _sub_textures = None

    def __init__(self):
        self._sub_textures = TextureList()

    def getClass(self):
        return self._class

    def getName(self):
        return self._name

    def getAmount(self):
        return self._amount

    def getSubTextureCount(self):
        return len(self._textures)

    def getSubTexture(self, idx):
        return self._textures[idx]

    def getSubTextures(self):
        return self._textures


class VertexColorTexture(BaseTexture):
    """Vertex color texture"""
    _class = "Vertex Color"

textureClasses[VertexColorTexture._class] = VertexColorTexture


SUB_TEXTURE_TYPE = {
    0: 'AMBIENT',
    1: 'DIFFUSE',
    2: 'SPECULAR', 
    3: 'SHINE',
    4: 'SHINESTRENGTH',
    5: 'SELFILLUM',
    6: 'OPACITY',
    7: 'FILTERCOLOR',
    8: 'BUMP',
    9: 'REFLECT',
    10: 'REFRACT'
}

MAP_TYPE = {
    0: 'Explicit',
    1: 'Spherical',
    2: 'Cylindrical',
    3: 'Shrinkwrap',
    4: 'Screen',
}

FILTER = {
    0: 'Pyramidal',
    1: 'SAT',
}

class BitmapTexture(BaseTexture):
    """A bitmap material texture"""
    _class = 'Bitmap'
    _bitmap = ''
    _u_offset = _v_offset = _u_tiling = _v_tiling = _angle = 0.0
    _blur = _blur_offset = _noise = _noise_size = _noise_phase = 0.0
    _type = _map_type = _noise_level = _invert = _filter = _channel = 0 

    def getBitmapName(self):
        return self._bitmap

    def getType(self):
        return SUB_TEXTURE_TYPE[self._type]

    def getMapType(self):
        return MAP_TYPE[self._map_type]

    def getUVOffset(self):
        return (self._u_offset, self._v_offset)

    def getUVTiling(self):
        return (self._u_tiling, self._v_tiling)

    def getUVAngle(self):
        return self._angle

    def getUVBlur(self):
        return self._blur

    def getUVBlurOffset(self):
        return self._blur_offset

    def getNoiseAmount(self):
        return self._noise

    def getNoiseSize(self):
        return self._noise_size

    def getNoiseLevel(self):
        return self._noise_level

    def getNoisePhase(self):
        return self._noise_phase

    def getInvert(self):
        return self._invert

    def getFilter(self):
        return FILTER[self._filter]

    def getChannel(self):
        return self._channel

textureClasses[BitmapTexture._class] = BitmapTexture
