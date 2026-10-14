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

"""Transformation Matrix.

This transformation matrix class is specifically designed to allow
decomposition of a 3 x 4 transformation matrix such as used by 3D Studio Max
into components such as needed by a VRML Transform Node.

Example:

>>> source = [
        [-1.0, 0.0, 1.5099580252808664e-007],
        [-1.5099580252808664e-007, -4.3711388286737929e-008, -1.0],
        [6.6002361555513294e-015, -1.0, 4.3711388286737929e-008],
        [0.0, -0.048601854592561722, 2.1244541681397777e-009]]
>>> tm = TransformMatrix(source)
>>> print tm
rotation         -0.707107 0.707107 5.33851e-008 5.33851e-008
scale            1 1 1
scaleOrientation 0 0 1 0
translation      0 -0.0486019 2.12445e-009
# sign           1
>>> tm.getRotation()
(-0.70710676573223585, 0.70710679664085485, 5.3385076782389043e-008,
5.3385079115924909e-008)

TransformMatrix constructor:

    TransformMatrix(source)

        - source is a 3 x 4 transformation matrix, all values are floats.

TransformMatrix instances public methods:

    getDeterminantSign()
        The sign of the determinant
       
        If negative, there is a 'flip', or mirror, in the rotation. If so,
        negate the scale factors, and change the direction of any affected
        face set. See:
            http://www.geocrawler.com/archives/3/297/1996/6/50/1691623/
   
    getRotation()
        Rotation axis and angle
   
    getScale()
        X, Y, and Z scale factors
   
    getScaleOrientation()
        Rotation axis and angle of scale
   
    getTranslation()
        X, Y and Z translation component

    __str__()
        String representation suitable for inclusion in a VRML 97 file.

"""

from Numeric import *

# Handy constants
X, Y, Z, W = range(4)

def quatToAxisAndAngle(quat):
    from math import acos
    # First normalize
    quat = array(quat, Float)
    norm = sqrt(add.reduce(quat ** 2))
    assert norm, "Cannot normalize the quaternion!"
    quat /= norm 
    angle = acos(quat[3])
    s = sin(angle)
    if s:
        return quat[:3] / s, 2.0 * angle
    else:
        return array((0, 0, 1), Float), 0.0

class TransformMatrix:
    """Here goes, python implementation of a transformation matrix.

    This includes decomposition of the matrix into it's VRML components.
    Based on C code from Graphic Gems IV, Ken Shoemake, 1993.

    http://www.acm.org/pubs/tog/GraphicsGems/gemsiv/polar_decomp/    

    """

    def __init__(self, source):
        """Create a matrix from a 3D studio Max transformation matrix."""
        self._matrix = identity(4, Float)
        self._matrix[:3] = transpose(source)
        self._decompose()
        self._toVRML()

    #
    # Public accessors
    #

    def getTranslation(self):
        """X, Y and Z translation component"""
        return self._translation

    def getRotation(self):
        """Rotation axis and angle"""
        return self._rotation

    def getScale(self):
        """X, Y, and Z scale factors"""
        return self._scaleFactors

    def getScaleOrientation(self):
        """Rotation axis and angle of scale"""
        return self._scaleOrientation

    def getDeterminantSign(self):
        """The sign of the determinant

        If negative, there is a 'flip', or mirror, in the rotation. If so,
        negate the scale factors, and change the direction of any affected
        face set. See:
            http://www.geocrawler.com/archives/3/297/1996/6/50/1691623/

        ASE exports already do this negation, and cause the face indexes to be
        reversed when exporting nodes.

        """
        return int(self._determinant_sign)

    def __str__(self):
        return '\n'.join((
            "rotation         %s" % ' '.join(
                ['%g' % f for f in self.getRotation()]),
            "scale            %s" % ' '.join(
                ['%g' % f for f in self.getScale()]),
            "scaleOrientation %s" % ' '.join(
                ['%g' % f for f in self.getScaleOrientation()]),
            "translation      %s" % ' '.join(
                ['%g' % f for f in self.getTranslation()]),
            "# sign           %i" % self.getDeterminantSign()))

    #
    # Decomposition
    #

    def _decompose(self):
        """Decompose matrix into translation, rotation and scale

        Decompose 4x4 affine matrix as TFRUK(U transpose), into translation
        components, rotation, scale rotation, scale factors, and sign of the
        determinant.
        
        Assumes matrix transforms column vectors in right-handed coordinates.
        See Ken Shoemake and Tom Duff. Matrix Animation and Polar
        Decomposition. Proceedings of Graphics Interface 1992.

        """
        self._translation = self._matrix[:W,W]

        self._determinant_sign, self._rotation, S = self._polar_decomp()
        self._scaleFactors, self._scaleOrientation = self._spect_decomp(S)
        self._snuggle()
        if self._scaleFactors == (1, 1, 1):
            # Compensate for our high precision; scale orientation tends to be
            # at odd angles (while the C code doesn't, because it only uses C
            # floats for its matrices, but python and Numeric use C doubles).
            self._scaleOrientation = array((0.0, 0.0, 0.0, 1.0))

    def _toVRML(self):
        """Clean up the found numbers.

        - Convert all to tuples.

        - VRML expects rotations to be of the order axis, rot; convert from
          quaternions.

        """

        self._translation = tuple(self._translation.tolist())
        self._scaleFactors = tuple(self._scaleFactors.tolist())

        axis, rot = quatToAxisAndAngle(self._rotation)
        self._rotation = tuple(axis.tolist()) + (rot,)

        axis, rot = quatToAxisAndAngle(self._scaleOrientation)
        self._scaleOrientation = tuple(axis.tolist()) + (rot,)

    #
    # Polar Decomposition
    #
    
    def _polar_decomp(self, TOL=1.0e-6):
        """Polar Decomposition of 3x3 matrix in 4x4, M = QS.

        See Nicholas Higham and Robert S. Schreiber, Fast Polar Decomposition
        of An Arbitrary Matrix, Technical Report 88-942, October 1988,
        Department of Computer Science, Cornell University.

        """
        Mk = transpose(self._matrix[:3,:3])
        M_one = self._norm_one(Mk)
        M_inf = self._norm_inf(Mk)
        while 1:
            MadjTk = self._adjoint_transpose(Mk)
            det = dot(Mk[0], MadjTk[0])
            if det == 0.0:
                Mk = self._do_rank2(Mk, MadjTk)
                break
            MadjT_one = self._norm_one(MadjTk)
            MadjT_inf = self._norm_inf(MadjTk)
            gamma = sqrt(
                sqrt((MadjT_one * MadjT_inf) / (M_one * M_inf)) /
                abs(det))
            g1 = gamma * 0.5
            g2 = 0.5 / (gamma * det)
            Ek = array(Mk)
            Mk = (g1 * Mk) + (g2 * MadjTk)
            Ek -= Mk
            E_one = self._norm_one(Ek)
            M_one = self._norm_one(Mk)
            M_inf = self._norm_inf(Mk)

            if not (E_one > (M_one * TOL)):
                break

        Q = identity(4, Float)
        Q[:3, :3] = transpose(Mk)

        S = dot(Mk, self._matrix[:3, :3])
        for i in range(3):
            for j in range(i, 3):
                S[i,j] = S[j,i] = 0.5 * (S[i,j] + S[j,i])                

        if det < 0:
            Q[:3,:3] = -Q[:3,:3]
            sign = -1
        else:
            sign = 1

        return sign, self._quat_from_matrix(Q), S


    #
    # Spectral Decomposition
    #
    
    def _spect_decomp(self, S):
        """Spectral decomposition of symmetric positive semi-definite S

        Returns rotation and scale factors, so that if K is a diagonal matrix
        of the scale factors, then S = rotation * K (scale transpose). Uses
        Jacobi method. See Gene H. Golub and Charles F. Van Loan. Matrix
        Computations. Hopkins 1983.

        """
        U = identity(4, Float)
        Diag = diagonal(S)
        OffD = array((S[Y, Z], S[Z, X], S[X, Y]))
        nxt = (Y, Z, X)

        for sweep in range(20):
            sm = add.reduce(fabs(OffD))
            if sm == 0.0:
                break
            for i in range(Z, X - 1, -1):
                p = nxt[i]
                q = nxt[p]
                fabsOffDi = abs(OffD[i])
                g = 100.0 * fabsOffDi
                if fabsOffDi:
                    h = Diag[q] - Diag[p]
                    fabsh = abs(h)
                    if fabsh + g == fabsh: # This is always false?
                        t = OffD[i] / h
                    else:
                        theta = 0.5 * h / OffD[i]
                        t = 1.0 / (abs(theta) + sqrt(theta * theta + 1.0))
                        if theta < 0: t = -t
                    c = 1.0 / sqrt(t * t + 1.0)
                    s = t * c
                    tau = s / (c + 1.0)
                    ta = t * OffD[i]
                    OffD[i] = 0.0
                    Diag[p] -= ta
                    Diag[q] += ta
                    OffDq = OffD[q]
                    OffD[q] -= s * (OffD[p] + tau * OffD[q])
                    OffD[p] += s * (OffDq   - tau * OffD[p])
                    for j in range(Z, X - 1, -1):
                        a = U[j, p]
                        b = U[j, q]
                        U[j, p] = U[j, p] - s * (b + tau * a)
                        U[j, q] = U[j, q] + s * (a - tau * b)

        
        return Diag, self._quat_from_matrix(U)

    #
    # Spectral Axis Adjustment
    #

    def _snuggle(self, SQRTHALF=sqrt(0.5)):
        """Spectral Axis Adjustment
        
        Given the scale orientation, q, and a scale factor, k, find a unit
        quaternion, p, which permutes the axes and turns freely in the plane
        of duplicate scale factors, such that q p has the largest possible w
        component, i.e. the smallest possible angle. Permutes k's components
        to go with q p instead of q. See Ken Shoemake and Tom Duff. Matrix
        Animation and Polar Decomposition. Proceedings of Graphics Interface
        1992. Details on p. 262-263.
        
        """
        q = self._scaleOrientation
        k = self._scaleFactors
        turn = -1
        if k[X] == k[Y]:
            if   k[X] == k[Z]: turn = W
            else:              turn = Z
        else:
            if   k[X] == k[Z]: turn = Y
            elif k[Y] == k[Z]: turn = X
        if turn > -1:
            qxtoz = array((0, SQRTHALF, 0, SQRTHALF), Float)
            qytoz = array((SQRTHALF, 0, 0, SQRTHALF), Float)
            qppmm = array(( 0.5, 0.5,-0.5,-0.5), Float)
            qpppp = array(( 0.5, 0.5, 0.5, 0.5), Float)
            qmpmm = array((-0.5, 0.5,-0.5,-0.5), Float)
            qpppm = array(( 0.5, 0.5, 0.5,-0.5), Float)
            q0001 = array(( 0.0, 0.0, 0.0, 1.0), Float)
            q1000 = array(( 1.0, 0.0, 0.0, 0.0), Float)

            if turn == X:
                qtoz = qxtoz
                q = self._quat_mult(q, qtoz)
                k[X], k[Z] = k[Z], k[X]
            elif turn == Y:
                qtoz = qytoz
                q = self._quat_mult(q, qtoz)
                k[Y], k[Z] = k[Z], k[Y]
            elif turn == Z:
                qtoz = q0001
            else:
                self._scaleOrientation = self._quat_mult(q, self._quat_conj(q))
                return

            q = self._quat_conj(q)
            mag = array((
                (q[Z] * q[Z]) + (q[W] * q[W]) - 0.5,
                (q[X] * q[Z]) - (q[Y] * q[W]),
                (q[Y] * q[Z]) + (q[X] * q[W])))
            neg = zeros(3)
            for i in range(3):
                neg[i] = (mag[i] < 0)
            mag = fabs(mag)
            win = 2 - argmax(mag[::-1]) # highest index, starting from last
            if win == 0:
                if neg[0]: p = q1000
                else:      p = q0001
            elif win == 1:
                if neg[1]: p = qppmm
                else:
                    p = qpppp
                    k = concatenate((k[3:], k[:3]))
            else:
                if neg[2]: p = qmpmm
                else:
                    p = qpppm
                    k = concatenate((k[1:], k[:1]))
            qp = self._quat_mult(q, p)
            t = sqrt(mag[win] + 0.5)
            p = self._quat_mult(p, array((0.0, 0.0, -qp[Z] / t, qp[W] / t)))
            p = self._quat_mult(qtoz, self._quat_conj(p))
        else:
            qa = array(q)
            pa = zeros(4, Float)
            neg = zeros(4)
            par = 0
            for i in range(4):
                neg[i] = (qa[i] < 0)
                par ^= neg[i]
            qa = fabs(qa)
            
            # Find two largest components, indices in hi and lo (counting from
            # end)
            hi = 3 - argmax(qa[::-1])
            remainder = array(qa)
            remainder[hi] = 0.0
            lo = 3 - argmax(remainder[::-1])
            if lo == hi: lo = hi - 1
            all = add.reduce(qa) * 0.5
            two = (qa[hi] + qa[lo]) * SQRTHALF
            big = qa[hi]
            if all > two:
                if all > big: # all
                    for i in range(4):
                        pa[i] = neg[i] and -0.5 or 0.5
                    if par: k = concatenate((k[1:], k[:1]))
                    else:   k = concatenate((k[3:], k[:3]))
                else: # big
                    pa[hi] = neg[hi] and -1.0 or 1.0
            else:
                if two > big: # two
                    pa[hi] = neg[hi] and -SQRTHALF or SQRTHALF
                    pa[lo] = neg[lo] and -SQRTHALF or SQRTHALF
                    if lo > hi: lo, hi = hi, lo
                    if hi == W:
                        hi = (1, 2, 0)[lo]
                        lo = 3 - hi - lo
                    k[hi], k[lo] = k[lo], k[hi]
                else: # big
                    pa[hi] = neg[hi] and -1.0 or 1.0
            p = -pa
            p[W] *= -1

        self._scaleFactors = k
        self._scaleOrientation = self._quat_mult(self._scaleOrientation, p)
        
    #
    # Decomposition support
    #
    
    def _norm_one(self, matrix):
        """Compute the 1 norm of matrix"""
        return self._norm_inf(transpose(matrix))

    def _norm_inf(self, matrix):
        """Compute the infinity norm of matrix"""
        return max(add.reduce(fabs(matrix[0])), add.reduce(fabs(matrix[1])),
                   add.reduce(fabs(matrix[2])))

    def _vcross(self, va, vb):
        """product of length 3 vectors va and vb"""
        return array((
            va[1] * vb[2] - va[2] * vb[1],
            va[2] * vb[0] - va[0] * vb[2],
            va[0] * vb[1] - va[1] * vb[0]))

    def _adjoint_transpose(self, matrix):
        """Return transpose of inverse of matrix times determinant of matrix"""
        return array((
            self._vcross(matrix[1], matrix[2]),
            self._vcross(matrix[2], matrix[0]),
            self._vcross(matrix[0], matrix[1])))

    def _find_max_col(self, matrix):
        """Return index of column of matrix containing maximum abs entry

        returns -1 if M=0

        """        
        col = -1
        mx = 0.0
        for i in range(matrix.shape[1]):
            colmax = max(fabs(matrix[:,i]))
            if colmax > mx:
                mx = colmax
                col = i
        return col

    def _make_reflector(self, v):
        "vector for Household reflection to zero all v components but first"
        s = sqrt(dot(v, v))
        u = array(v)
        if v[2] < 0:
            u[2] -= s
        else:
            u[2] += s
        s = sqrt(2.0 / dot(u, u))
        return u * s

    def _reflect_cols(self, matrix, u):
        """Apply Householder reflection u to column vectors of matrix"""
        M = array(matrix)
        for i in range(M.shape[1]):
            s = dot(u, M[:, i])
            M[:, i] -= (u * s)    
        return M

    def _reflect_rows(self, matrix, u):
        """Apply Householder reflection u to row vectors of matrix"""
        M = array(matrix)
        for i in range(M.shape[0]):
            s = dot(u, M[i])
            M[i] -= (u * s)    
        return M

    def _do_rank1(self, matrix):
        """Orthogonal factor of rank 1 (or less) matrix"""
        Q = identity(3, Float)
        col = self._find_max_col(matrix)
        if col < 0:
            return Q
        v1 = self._make_reflector(matrix[:, col])
        matrix = self._reflect_cols(v1)
        v2 = self._make_reflector(matrix[2])
        matrix = self._reflect_rows(v2)
        s = self._matrix[2, 2]
        if s < 0:
            Q[2, 2] = -1.0
        Q = self._reflect_cols(Q, v1)
        Q = self._reflect_rows(Q, v2)
        return Q

    def _do_rank2(self, matrix, MadjT):
        "Orthogonal factor of rank 2 (or less) matrix using adjoint transpose"
        col = self._find_max_col(MadjT)
        if col < 0:
            return self._do_rank1(matrix)
        v1 = self._make_reflector(MadjT[:, col])
        M = self._reflect_cols(matrix, v1)
        v2 = self._make_reflector(self._vcross(matrix[0], matrix[1]))
        matrix = self._reflect_rows(matrix, v2)

        w, z = self._matrix[0,:1]
        y, z = self._matrix[1,:1]
        Q = zeros((3, 3), Float)
        if w * z > x * y:
            c = z + w
            s = y - x
            d = sqrt(c * c + s * s)
            c = c / d
            s = s / d
            Q[0, 0] = Q[1, 1] = c
            Q[1, 0] = s
            Q[0, 1] = -Q[1, 0]
        else:
            c = z - w
            s = y + x
            d = sqrt(c * c + s * s)
            c = c / d
            s = s / d
            Q[1, 1] = c
            Q[0, 0] = -Q[1, 1]
            Q[0, 1] = Q[1, 0] = s
        Q[0, 2] = Q[2, 0] = Q[1, 2] = Q[2, 1] = 0.0
        Q[2, 2] = 1.0
        Q = self._reflect_cols(Q, v1)
        Q = self._reflect_rows(Q, v2)
        return Q

    #
    # Quaternion support
    #
    
    def _quat_from_matrix(self, mat):
        """Construct a unit quaternion from rotation matrix.

        Assumes matrix is used to multiply column vector on the left:
        vnew = mat vold. Works correctly for right-handed coordinate system
        and right-handed rotations. Translation and perspective components
        ignored.

        This algorithm avoids near-zero divides by looking for a large
        component - first w, then x, y, or z.  When the trace is greater than
        zero, |w| is greater than 1/2, which is as small as a largest
        component can be. Otherwise, the largest diagonal entry corresponds to
        the largest of |x|, |y|, or |z|, one of which must be larger than |w|,
        and at least 1/2.
        
        """   
        tr = add.reduce(diagonal(mat)[:-1])
        qu = zeros(4, Float)
        if tr > 0:
            s = sqrt(tr + mat[W,W])
            qu[W] = s * 0.5
            s = 0.5 / s
            qu[X] = (mat[Z, Y] - mat[Y, Z]) * s
            qu[Y] = (mat[X, Z] - mat[Z, X]) * s
            qu[Z] = (mat[Y, X] - mat[X, Y]) * s
        else:
            h = X
            if mat[Y, Y] > mat[X, X]: h = Y
            if mat[Z, Z] > mat[h, h]: h = Z

            if   h == X: i, j, k = X, Y, Z
            elif h == Y: i, j, k = Y, Z, X
            elif h == Z: i, j, k = Z, X, Y

            s = sqrt((mat[i, i] - (mat[j, j] + mat[k, k])) + mat[W, W])
            qu[i] = s * 0.5
            s = 0.5 / s
            qu[j] = (mat[i, j] + mat[j, i]) * s
            qu[k] = (mat[k, i] + mat[i, k]) * s
            qu[W] = (mat[k, j] - mat[j, k]) * s

        if mat[W, W] != 1.0:
            qu *= 1 / sqrt(mat[W, W])

        return qu

    def _quat_mult(self, q, p):
        """Return quaternion product q * p.

        Note: order is important! To combine rotations, use the product
        Mul(p, q) which gives the effect of rotating by qFirst then
        qSecond.

        """
        return array((
            (q[W] * p[X]) + (q[X] * p[W]) + (q[Y] * p[Z]) - (q[Z] * p[Y]),
            (q[W] * p[Y]) + (q[Y] * p[W]) + (q[Z] * p[X]) - (q[X] * p[Z]),
            (q[W] * p[Z]) + (q[Z] * p[W]) + (q[X] * p[Y]) - (q[Y] * p[X]),
            (q[W] * p[W]) - (q[X] * p[X]) - (q[Y] * p[Y]) - (q[Z] * p[Z])))

    def _quat_conj(self, q):
        """Return conjugate of quaternion."""
        return concatenate((-q[:3], q[-1:]))
