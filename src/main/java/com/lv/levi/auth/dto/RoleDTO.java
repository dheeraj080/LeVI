package com.lv.levi.auth.dto;


import lombok.*;

import java.util.UUID;

@Getter
@Setter
@AllArgsConstructor
@NoArgsConstructor
@Builder
public class RoleDTO {

    private UUID id;
    private String name;
}
