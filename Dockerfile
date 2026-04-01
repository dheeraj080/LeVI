# ---- build stage ----
FROM maven:3-eclipse-temurin-25 AS builder
WORKDIR /app

# Cache deps layer (unchanged from yours — already optimal)
COPY pom.xml .
RUN --mount=type=cache,target=/root/.m2 mvn dependency:go-offline -B

# Parallel build threads shave time on multi-module projects
COPY src ./src
RUN --mount=type=cache,target=/root/.m2 \
    mvn package -Dmaven.test.skip=true -B -T1C

# Extract Spring Boot layertools — splits the fat jar into 4 ordered layers
RUN java -Djarmode=layertools -jar target/*.jar extract --destination target/extracted

# ---- runtime stage ----
FROM eclipse-temurin:25-jre-alpine AS runtime
WORKDIR /app

# Ensure we have a system group/users
RUN addgroup -S spring && adduser -S spring -G spring

# Copying layers - explicitly keeping the folder structure helps JarLauncher
COPY --from=builder --chown=spring:spring /app/target/extracted/dependencies/ ./
COPY --from=builder --chown=spring:spring /app/target/extracted/spring-boot-loader/ ./
COPY --from=builder --chown=spring:spring /app/target/extracted/snapshot-dependencies/ ./
COPY --from=builder --chown=spring:spring /app/target/extracted/application/ ./

USER spring:spring

# Standard Spring Boot port is 8080; 4005 is specific to your app, which is fine.
EXPOSE 4005

ENTRYPOINT ["java", \
  "-XX:TieredStopAtLevel=1", \
  "-XX:+UseContainerSupport", \
  "org.springframework.boot.loader.launch.JarLauncher"]