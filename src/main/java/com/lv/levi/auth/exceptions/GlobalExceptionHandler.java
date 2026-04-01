package com.lv.levi.auth.exceptions;

import com.lv.levi.auth.dto.ApiError;
import com.lv.levi.auth.dto.ErrorResponse;
import io.jsonwebtoken.ExpiredJwtException;
import jakarta.servlet.http.HttpServletRequest;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import org.springframework.http.HttpStatus;
import org.springframework.http.ResponseEntity;
import org.springframework.security.authentication.BadCredentialsException;
import org.springframework.security.authentication.CredentialsExpiredException;
import org.springframework.security.core.AuthenticationException;
import org.springframework.security.core.userdetails.UsernameNotFoundException;
import org.springframework.web.bind.annotation.ExceptionHandler;
import org.springframework.web.bind.annotation.RestControllerAdvice;

@RestControllerAdvice
public class GlobalExceptionHandler {

    private final Logger log = LoggerFactory.getLogger(GlobalExceptionHandler.class);

    @ExceptionHandler(ResourceNotFoundException.class)
    public ResponseEntity<ErrorResponse> handleResourceNotFoundException(ResourceNotFoundException exception) {
        ErrorResponse error = new ErrorResponse(exception.getMessage(), HttpStatus.NOT_FOUND, 404);
        return ResponseEntity.status(HttpStatus.NOT_FOUND).body(error);
    }

    @ExceptionHandler(IllegalArgumentException.class)
    public ResponseEntity<ErrorResponse> handleIllegalArgumentException(IllegalArgumentException exception) {
        ErrorResponse error = new ErrorResponse(exception.getMessage(), HttpStatus.BAD_REQUEST, 400);
        return ResponseEntity.status(HttpStatus.BAD_REQUEST).body(error);
    }

    // Consolidated Auth Handler: Removed the redundant 'handleBadCredentials' method
    // and grouped all security-related exceptions here.
    @ExceptionHandler({
            UsernameNotFoundException.class,
            BadCredentialsException.class,
            CredentialsExpiredException.class,
            AuthenticationException.class
    })
    public ResponseEntity<ApiError> handleAuthException(Exception e, HttpServletRequest request) {
        log.error(e.getMessage(), e.getCause());
        // We use 401 UNAUTHORIZED for credentials issues rather than 400 BAD REQUEST
        HttpStatus status = HttpStatus.UNAUTHORIZED;

        var apiError = ApiError.of(
                status.value(),
                "Authentication Failed",
                e.getMessage(),
                request.getRequestURI()
        );

        return ResponseEntity.status(status).body(apiError);
    }

    @ExceptionHandler(ExpiredJwtException.class)
    public ResponseEntity<ApiError> handleExpiredJwt(ExpiredJwtException e, HttpServletRequest request) {
        var apiError = ApiError.of(
                HttpStatus.UNAUTHORIZED.value(),
                "Token Expired",
                "Your session has expired. Please login again.",
                request.getRequestURI()
        );
        return ResponseEntity.status(HttpStatus.UNAUTHORIZED).body(apiError);
    }

    @ExceptionHandler(Exception.class)
    public ResponseEntity<ErrorResponse> handleGlobalException(Exception exception) {
        // 1. Log the full stack trace to your terminal/Docker logs
        log.error("Global Exception caught: ", exception);

        // 2. Temporarily return the ACTUAL message to Postman for debugging
        ErrorResponse error = new ErrorResponse(
                exception.getMessage(), // Changed from "An unexpected error occurred"
                HttpStatus.INTERNAL_SERVER_ERROR,
                500
        );
        return ResponseEntity.status(HttpStatus.INTERNAL_SERVER_ERROR).body(error);
    }
}