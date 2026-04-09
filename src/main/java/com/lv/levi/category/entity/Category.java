package com.lv.levi.category.entity;

import com.lv.levi.auth.entity.User;
import jakarta.persistence.*;
import lombok.*;

import java.time.Instant;
import java.util.UUID;

@Entity
@Table(name = "categories")
@Getter
@Setter
@AllArgsConstructor
@NoArgsConstructor
@Builder
public class Category {

    @Id
    @GeneratedValue(strategy = GenerationType.UUID)
    @Column(name = "category_id")
    private UUID id;

    private String name;

    @Enumerated(EnumType.STRING)
    private CategoryType type;

    private String icon;

    @Builder.Default
    @Column(updatable = false, nullable = false)
    private Instant createdAt = Instant.now();

    @Builder.Default
    private Instant updatedAt = Instant.now();

    @ManyToOne(fetch = FetchType.LAZY)
    @JoinColumn(
            name = "profile_user_id",      // Name in the 'categories' table
            referencedColumnName = "user_id", // Name in the 'users' table
            nullable = false
    )
    private User user;

    @Builder.Default
    private boolean deleted = false;

}
