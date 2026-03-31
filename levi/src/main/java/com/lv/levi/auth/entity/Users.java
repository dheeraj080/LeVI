package com.lv.levi.auth.entity;

import jakarta.persistence.*;
import lombok.*;
import org.hibernate.annotations.CreationTimestamp;
import org.hibernate.annotations.UpdateTimestamp;
import org.hibernate.validator.constraints.Length;

import java.time.LocalDate;
import java.util.UUID;

@Entity
@Table(name = "users")
@AllArgsConstructor
@NoArgsConstructor
@Getter
@Setter
@ToString
@Builder
public class Users {

    @Id
    @GeneratedValue(strategy = GenerationType.UUID)
    private UUID id;

    private String username;
    private String firstName;
    private String lastName;

    private String phone;

    @Column(unique = true)
    private String email;

    @Length(min = 8)
    private String password;

    private String profilePicture;

    @Column(updatable = false)
    @CreationTimestamp
    private LocalDate createAt;

    @Column(updatable = true)
    @UpdateTimestamp
    private LocalDate updateAt;

    private Boolean isActive;
    private String activationCode;

    @PrePersist
    public void prePersist(){
        if(this.isActive == null){
            isActive = false;
        }
    }
}
