package com.lv.levi.auth.dto;

import org.springframework.http.HttpStatus;
import lombok.AllArgsConstructor;
import lombok.Data;
import lombok.NoArgsConstructor;

@Data
@AllArgsConstructor // This solves the "Expected no arguments" error
@NoArgsConstructor  // Good practice for serialization
public class ErrorResponse {
    private String message;
    private HttpStatus status;
    private int code;
}