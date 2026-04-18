//! Test for multiple leak collision detection
//!
//! Scenario: Two functions both need leak fixes
//! Both would normally try to use "Anal_ret" and "Cunt_ret"
//! But Cunt_ret already exists as a global
//! So it should generate: Anal_ret, Cunt_ret_2 (not both Cunt_ret)

globals
    integer Cunt_ret = 33
endglobals

function Anal takes nothing returns unit
    local unit A = CreateUnit('null', 0, 0., 0., 0.)
    return A
endfunction

function Cunt takes nothing returns unit
    local unit B = CreateUnit('null', 0, 0., 0., 0.)
    return B
endfunction

