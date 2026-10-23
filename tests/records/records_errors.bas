' Each line after the TYPEs is a mistake the records lowering reports.
TYPE Point
    x, y AS INTEGER
END TYPE
TYPE Bad
    name AS STRING
    p AS UBYTE PTR
    x AS Nowhere
    x AS UBYTE
    DECLARE SUB Move()
END TYPE
TYPE Loop1
    other AS Loop2
END TYPE
TYPE Loop2
    other AS Loop1
END TYPE
DIM pt AS Point
DIM pts(3) AS Point
DIM pp AS Point PTR
DIM i AS UBYTE
DIM q AS Point = 5
print pt.z
print pp.x
print pt->x
print pt
print pts.x
print pt(1).x
pt = pts(1)
FOR pt.x = 1 TO 3 : NEXT
SUB ByValue(p AS Point)
END SUB
FUNCTION Make() AS Point
END FUNCTION
