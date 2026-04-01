package com.lv.levi.auth.service.impl;

import com.lv.levi.auth.dto.UserDTO;
import com.lv.levi.auth.repository.UserRepository;
import com.lv.levi.auth.service.AuthService;
import com.lv.levi.auth.service.UserService;
import lombok.AllArgsConstructor;
import org.springframework.stereotype.Service;

@Service
@AllArgsConstructor
public class AuthServiceImpl implements AuthService {

    private final UserService userService;
    private final UserRepository userRepository; // Added to check for existing email

    @Override
    public UserDTO registerUser(UserDTO userDTO) {
        // 1. Validation Logic moved from UserServiceImpl
        if (userDTO.getEmail() == null || userDTO.getEmail().isBlank()) {
            throw new IllegalArgumentException("Email is Required");
        }
        if (userDTO.getPassword() == null || userDTO.getPassword().isBlank()) {
            throw new IllegalArgumentException("Password is Required");
        }
        if (userRepository.existsByEmail(userDTO.getEmail())) {
            throw new IllegalArgumentException("Email Already Exists");
        }

        // 2. Delegate creation to the user service
        return userService.createUser(userDTO);
    }


}