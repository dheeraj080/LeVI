package com.lv.levi.auth.service;

import com.lv.levi.auth.entity.ResetPasswordToken;
import com.lv.levi.auth.entity.User;
import com.lv.levi.auth.repository.ResetPasswordTokenRepository;
import com.lv.levi.auth.repository.UserRepository;
import lombok.RequiredArgsConstructor;
import lombok.extern.slf4j.Slf4j;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.security.SecureRandom;
import java.time.Instant;
import java.time.temporal.ChronoUnit;

@Slf4j
@Service
@RequiredArgsConstructor
public class ResetPasswordService {

    private final ResetPasswordTokenRepository tokenRepository;
    private final UserRepository userRepository;
    private final PasswordEncoder passwordEncoder;
    private final EmailService emailService;

    // SecureRandom is better for generating security codes than the default Random
    private final SecureRandom secureRandom = new SecureRandom();

    @Transactional
    public void sendResetCode(String email) {
        User user = userRepository.findByEmail(email)
                .orElseThrow(() -> new RuntimeException("User not found"));

        // 1. Clean up any existing codes for this user to avoid conflicts
        tokenRepository.deleteByUser(user);
        tokenRepository.flush();

        // 2. Generate a 6-digit numeric code (e.g., "054321")
        String code = String.format("%06d", secureRandom.nextInt(1000000));

        // 3. Save the code to the database
        ResetPasswordToken resetToken = ResetPasswordToken.builder()
                .code(code) // Using the 'token' field in the DB to store the numeric code
                .user(user)
                .expiryDate(Instant.now().plus(10, ChronoUnit.MINUTES)) // Codes should be short-lived
                .used(false)
                .build();

        tokenRepository.save(resetToken);

        // 4. Send the email
        String subject = "Your Password Reset Code";
        String body = """
                Hello,
                
                You requested a password reset. Please use the following 6-digit code to update your password:
                
                %s
                
                This code will expire in 10 minutes. If you did not request this, please ignore this email.
                """.formatted(code);

        emailService.sendEmail(email, subject, body);
        log.info("Reset code sent successfully to: {}", email);
    }

    @Transactional
    public void resetPassword(String email, String code, String newPassword) {
        // 1. Find the user by email first
        User user = userRepository.findByEmail(email)
                .orElseThrow(() -> new RuntimeException("Invalid code or email address"));

        // 2. Find the token associated with this specific user and code
        ResetPasswordToken resetToken = tokenRepository.findByCodeAndUser(code, user)
                .orElseThrow(() -> new RuntimeException("Invalid code or email address"));

        // 3. Standard validations
        if (resetToken.isExpired()) {
            throw new RuntimeException("The code has expired.");
        }
        if (resetToken.isUsed()) {
            throw new RuntimeException("This code has already been used.");
        }

        // 4. Update password and mark token used
        user.setPassword(passwordEncoder.encode(newPassword));
        userRepository.save(user);

        resetToken.setUsed(true);
        tokenRepository.save(resetToken);
    }
}