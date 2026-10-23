REM An IF whose branch only sets a variable nobody reads is empty after
REM optimisation and is dropped (W140) - but the call in its condition must
REM still run. Look for "call _Side" in _Test.
dim flag as ubyte
dim calls as ubyte
function Side() as ubyte
    calls = calls + 1
    return 1
end function
sub Test()
    flag = 0
    if Side() = 1 then flag = 1
end sub
Test()
poke 30000, calls
