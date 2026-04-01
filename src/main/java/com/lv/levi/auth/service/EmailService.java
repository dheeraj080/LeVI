package com.lv.levi.auth.service;

public interface EmailService {
    void sendEmail(String to, String subject, String body);
}