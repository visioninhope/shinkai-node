import axios from "axios";
import { AppDispatch } from "../store/index";
import {
  getPublicKey,
  useRegistrationCode,
  pingAll,
  createRegistrationCode,
  registrationError,
} from "../store/actions";
import { ThunkAction } from "redux-thunk";
import { Action } from "redux";
import { RootState } from "../store";
import { AppThunk } from "../types";
import { ShinkaiMessageBuilderWrapper } from "../lib/wasm/ShinkaiMessageBuilderWrapper";
import { MergedSetupType } from "../pages/Connect";
import { useSelector } from "react-redux";
import { ApiConfig } from "./api_config";
import { SetupDetailsState } from "../store/reducers";

// Helper function to handle HTTP errors
export const handleHttpError = (response: any) => {
  if (response.status < 200 || response.status >= 300) {
    const error = response.data;
    throw new Error(
      `HTTP error: ${error.code}, ${error.error}, ${error.message}`
    );
  }
};

export const fetchPublicKey = () => async (dispatch: AppDispatch) => {
  const apiEndpoint = ApiConfig.getInstance().getEndpoint();
  try {
    const response = await axios.get(`${apiEndpoint}/get_public_key`);
    dispatch(getPublicKey(response.data));
  } catch (error) {
    console.error("Error fetching public key:", error);
  }
};

export const submitRequestRegistrationCode =
  (
    identity_permissions: string,
    code_type = "profile",
    setupDetailsState: SetupDetailsState
  ) =>
  async (dispatch: AppDispatch) => {
    try {
      // TODO: refactor this
      let sender_profile_name = setupDetailsState.profile + "/device/" + setupDetailsState.registration_name;
      console.log("sender_profile_name:", sender_profile_name);
      const messageStr = ShinkaiMessageBuilderWrapper.request_code_registration(
        setupDetailsState.myEncryptionSk,
        setupDetailsState.myIdentitySk,
        setupDetailsState.node_encryption_pk,
        identity_permissions,
        code_type,
        sender_profile_name,
        setupDetailsState.shinkai_identity
      );

      const message = JSON.parse(messageStr);
      console.log("Message:", message);

      const apiEndpoint = ApiConfig.getInstance().getEndpoint();
      const response = await axios.post(
        `${apiEndpoint}/v1/create_registration_code`,
        message
      );

      handleHttpError(response);
      dispatch(createRegistrationCode(response.data.code));
    } catch (error) {
      console.error("Error creating registration code:", error);
    }
  };

export const submitRegistrationCode =
  (setupData: MergedSetupType): AppThunk =>
  async (dispatch: AppDispatch) => {
    try {
      const messageStr = ShinkaiMessageBuilderWrapper.use_code_registration(
        setupData.myEncryptionSk,
        setupData.myIdentitySk,
        setupData.node_encryption_pk,
        setupData.registration_code,
        setupData.identity_type,
        setupData.permission_type,
        setupData.registration_name,
        "", // sender_profile_name: it doesn't exist yet in the Node
        setupData.shinkai_identity
      );

      const message = JSON.parse(messageStr);
      console.log("Message:", message);

      // Use node_address from setupData for API endpoint
      let response = await axios.post(
        `${setupData.node_address}/v1/use_registration_code`,
        message
      );

      handleHttpError(response);

      // Update the API_ENDPOINT after successful registration
      ApiConfig.getInstance().setEndpoint(setupData.node_address);

      dispatch(useRegistrationCode(setupData));

      return true;
    } catch (error) {
      console.log("Error using registration code:", error);
      if (error instanceof Error) {
        dispatch(registrationError(error.message));
      }
      return false;
    }
  };

export const pingAllNodes = () => async (dispatch: AppDispatch) => {
  const apiEndpoint = ApiConfig.getInstance().getEndpoint();
  try {
    const response = await axios.post(`${apiEndpoint}/ping_all`);
    handleHttpError(response);
    dispatch(pingAll(response.data.result));
  } catch (error) {
    console.error("Error pinging all nodes:", error);
  }
};
