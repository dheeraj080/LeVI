package com.lv.levi.auth.dto;

public record LoginRequest(
        String email,
        String password
) {

}
