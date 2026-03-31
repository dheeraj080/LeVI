package com.lv.levi.auth.service;

import com.lv.levi.auth.dto.UsersDto;
import com.lv.levi.auth.entity.Users;
import com.lv.levi.auth.repository.UserRepository;
import lombok.RequiredArgsConstructor;
import org.springframework.stereotype.Service;
import org.springframework.transaction.annotation.Transactional;

import java.util.UUID;

@Service
@RequiredArgsConstructor
public class UserService {

    private final UserRepository userRepository;
    private final EmailService emailService;

    @Transactional
    public UsersDto registerUser(UsersDto usersDto) {
        Users newUsers = toEntity(usersDto);
        newUsers.setActivationCode(UUID.randomUUID().toString());
        Users savedUser = userRepository.save(newUsers);

        // activation email
        String activationLink = "http://localhost:4000/api/v1/activate?code=" + savedUser.getActivationCode();
        String subject = "Activation Code for your LeVI account";
        String body = "Click on the following link to activate your LeVI account :\n" + activationLink;
        emailService.sendEmail(savedUser.getEmail(), subject, body);

        return toDto(savedUser);
    }

    public Users toEntity(UsersDto usersDto) {
        return Users.builder()
                .username(usersDto.getUsername())
                .firstName(usersDto.getFirstName())
                .lastName(usersDto.getLastName())
                .phone(usersDto.getPhone())
                .email(usersDto.getEmail())
                .password(usersDto.getPassword())
                .profilePicture(usersDto.getProfilePicture())
                .build();
    }

    public UsersDto toDto(Users user) {
        return UsersDto.builder()
                .id(user.getId())
                .username(user.getUsername())
                .firstName(user.getFirstName())
                .lastName(user.getLastName())
                .email(user.getEmail())
                .phone(user.getPhone())
                .createAt(user.getCreateAt())
                .updateAt(user.getUpdateAt())
                .build();
    }

    public boolean activateUser(String ActivationCode) {
        return userRepository.findByActivationCode(ActivationCode)
                .map(users -> {
                    users.setIsActive(true);
                    userRepository.save(users);
                    return true;
                })
                .orElse(false);
    }
}

