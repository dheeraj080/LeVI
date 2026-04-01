package com.lv.levi.auth.dto;

public record TokenResponse(
        String accessToken,
        String refreshToken,
        long expiresIn,
        String tokenType,
        UserDTO user
) {
    // Static factory method for cleaner instantiation
    public static TokenResponse of(String accessToken, String refreshToken, long expiresIn, UserDTO user) {
        return new TokenResponse(accessToken, refreshToken, expiresIn, "Bearer", user);
    }
}