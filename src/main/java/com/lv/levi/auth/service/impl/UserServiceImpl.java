package com.lv.levi.auth.service.impl;

import com.lv.levi.auth.dto.UserDTO;
import com.lv.levi.auth.entity.Provider;
import com.lv.levi.auth.entity.Role;
import com.lv.levi.auth.entity.User;
import com.lv.levi.auth.exceptions.ResourceNotFoundException;
import com.lv.levi.auth.helper.UserHelper;
import com.lv.levi.auth.repository.RoleRepository;
import com.lv.levi.auth.repository.UserRepository;
import com.lv.levi.auth.service.EmailService;
import com.lv.levi.auth.service.UserService;
import jakarta.transaction.Transactional;
import lombok.RequiredArgsConstructor;
import lombok.extern.slf4j.Slf4j;
import org.modelmapper.ModelMapper;
import org.springframework.security.crypto.password.PasswordEncoder;
import org.springframework.stereotype.Service;

import java.util.Collections;
import java.util.UUID;
import java.util.stream.Collectors;

@Service
@RequiredArgsConstructor
@Slf4j
public class UserServiceImpl implements UserService {

    private final UserRepository userRepository;
    private final RoleRepository roleRepository;
    private final ModelMapper modelMapper;
    private final PasswordEncoder passwordEncoder;
    private final EmailService emailService;

    @Override
    @Transactional
    public UserDTO createUser(UserDTO userDTO) {
        // The validation is now handled in AuthServiceImpl,
        // so we get straight to the mapping and logic.

        // 1. Map DTO to Entity
        User user = modelMapper.map(userDTO, User.class);

        // 2. Security & Defaults
        user.setPassword(passwordEncoder.encode(userDTO.getPassword()));
        user.setProvider(userDTO.getProvider() != null ? userDTO.getProvider() : Provider.LOCAL);
        user.setEnabled(false);

        // Generate the Activation Code
        String code = UUID.randomUUID().toString();
        user.setActivationCode(code);

        // 3. Assign Default Role
        Role defaultRole = roleRepository.findByName("ROLE_USER")
                .orElseThrow(() -> new ResourceNotFoundException("Default Role 'ROLE_USER' not found"));
        user.setRoles(Collections.singleton(defaultRole));

        // 4. Persistence
        User savedUser = userRepository.save(user);

        // 5. Email Notification
        String activationUrl = "http://localhost:4000/api/v1/auth/activate?code=" + code;
        String subject = "Activate your LeVI Account";
        String body = "Welcome! Please activate your account: " + activationUrl;
        emailService.sendEmail(savedUser.getEmail(), subject, body);

        return modelMapper.map(savedUser, UserDTO.class);
    }

    @Override
    @Transactional
    public boolean activateUser(String code) {
        // 1. Find user by activation code
        User user = userRepository.findByActivationCode(code)
                .orElseThrow(() -> new ResourceNotFoundException("Invalid or expired activation code"));
        user.setEnabled(true);
        user.setActivationCode(null);
        userRepository.save(user);

        log.info("User with email {} has been successfully activated.", user.getEmail());
        return false;
    }

    @Override
    public UserDTO getUserByEmail(String email) {
        User user = userRepository.findByEmail(email)
                .orElseThrow(() -> new ResourceNotFoundException("User not found with email: " + email));
        return modelMapper.map(user, UserDTO.class);
    }

    @Override
    @Transactional
    public UserDTO updateUser(UserDTO userDTO, String userId) {
        UUID uId = UserHelper.parseUUID(userId);
        User existingUser = userRepository.findById(uId)
                .orElseThrow(() -> new ResourceNotFoundException("User not found with ID: " + userId));

        if (userDTO.getName() != null) existingUser.setName(userDTO.getName());
        if (userDTO.getImage() != null) existingUser.setImage(userDTO.getImage());
        if (userDTO.getProvider() != null) existingUser.setProvider(userDTO.getProvider());

        if (userDTO.getPassword() != null && !userDTO.getPassword().isBlank()) {
            existingUser.setPassword(passwordEncoder.encode(userDTO.getPassword()));

        }



        // FIX: Changed 'user' to 'existingUser'
        if (userDTO.getEnabled() != null) {
            existingUser.setEnabled(userDTO.getEnabled());
        }

        User updatedUser = userRepository.save(existingUser);
        return modelMapper.map(updatedUser, UserDTO.class);
    }

    @Override
    public void deleteUser(String userId) {
        UUID uId = UserHelper.parseUUID(userId);
        if (!userRepository.existsById(uId)) {
            throw new ResourceNotFoundException("User not found with ID: " + userId);
        }
        userRepository.deleteById(uId);
    }

    @Override
    public UserDTO getUserById(String userId) {
        UUID uId = UserHelper.parseUUID(userId);
        User user = userRepository.findById(uId)
                .orElseThrow(() -> new ResourceNotFoundException("User not found with ID: " + userId));
        return modelMapper.map(user, UserDTO.class);
    }

    @Override
    public Iterable<UserDTO> getAllUsers() {
        return userRepository.findAll()
                .stream()
                .map(user -> modelMapper.map(user, UserDTO.class))
                .collect(Collectors.toList());
    }
}