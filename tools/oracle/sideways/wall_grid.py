"""wall_grid.py: the axis (chassis frame) and angle of a tipab trial that stands the Scout on its tail (pitch -90) and then turns it
about the world vertical by psi, so that its top face (psi 315) or bottom face (psi 135) looks at the box's -x wall (docs/29 12bd).
  PYTHONPATH=. python3 wall_grid.py 315
The Scout's rest rotation is the one every box session starts from (rows are the chassis axes in the world)."""
import sys
from qmath import from_rows, qaxis, qconj, qmul, qrot, to_axis_angle

REST_ROWS = [[-0.72, 0, -0.7], [0.03, 1, -0.03], [0.7, -0.04, -0.72]]
QR = from_rows(REST_ROWS)


def axis_angle_for(psi, pitch=-90):
    qp = qmul(qmul(QR, qaxis((1, 0, 0), pitch)), qconj(QR))
    q = qmul(qaxis((0, 1, 0), psi), qp)
    aw, ang = to_axis_angle(q)
    return qrot(qconj(QR), aw), ang


if __name__ == "__main__":
    for psi in map(float, sys.argv[1:] or ["315", "135"]):
        print(psi, axis_angle_for(psi))
